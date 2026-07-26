#[cfg(desktop)]
use tauri::Manager;

/// What to do about NVIDIA's proprietary driver, as data rather than as env
/// writes, so the policy is testable without touching the process environment.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NvidiaRenderFix {
    /// Not the proprietary driver — WebKit's own defaults are correct.
    None,
    /// Wayland: keep the DMA-BUF renderer, drop NVIDIA's explicit sync.
    DisableExplicitSync,
    /// X11: GBM allocation fails outright, so fall back to software.
    DisableDmabufRenderer,
}

/// GTK talks Wayland only when it actually picks the Wayland backend.
/// `GDK_BACKEND=x11` runs under XWayland *inside* a Wayland session and needs
/// the X11 treatment, so an explicit backend request outranks the session vars.
#[cfg(target_os = "linux")]
fn prefers_wayland(
    gdk_backend: Option<&str>,
    wayland_display: Option<&str>,
    session_type: Option<&str>,
) -> bool {
    if let Some(backend) = gdk_backend.map(str::trim).filter(|b| !b.is_empty()) {
        // GDK_BACKEND is a comma-separated preference list; the first entry wins.
        return backend.split(',').next() == Some("wayland");
    }
    wayland_display.is_some_and(|v| !v.is_empty())
        || session_type.is_some_and(|v| v.eq_ignore_ascii_case("wayland"))
}

#[cfg(target_os = "linux")]
fn decide_nvidia_render_fix(has_nvidia: bool, wayland: bool) -> NvidiaRenderFix {
    match (has_nvidia, wayland) {
        (false, _) => NvidiaRenderFix::None,
        (true, true) => NvidiaRenderFix::DisableExplicitSync,
        (true, false) => NvidiaRenderFix::DisableDmabufRenderer,
    }
}

/// Linux + NVIDIA's proprietary driver needs help before GTK/WebKit starts, and
/// *which* help depends on the display server.
///
/// **Wayland.** WebKitGTK's DMA-BUF renderer is what gives the canvas GPU
/// compositing, and losing it is brutal: without it every frame moves the whole
/// canvas to the screen on the CPU — measured at ~51 ms for a 4800x2512 canvas,
/// i.e. 12.9 fps on a maximised window where Chrome runs the same build at 60.
/// The renderer is not itself broken here. What kills the client is NVIDIA's
/// explicit sync: the dmabuf buffer allocates fine, then the compositor tears
/// the connection down with `wp_linux_drm_syncobj_surface_v1: explicit sync is
/// used, but no acquire point is set`. NVIDIA's egl-wayland honours
/// `__NV_DISABLE_EXPLICIT_SYNC`, which falls back to implicit sync and keeps the
/// renderer alive — 12.9 fps -> 60.
///
/// This supersedes reading the same symptom as a `GLContext::~GLContext`
/// segfault. That crash is real but separate, and it is **accepted** here: with
/// the renderer enabled the web process still segfaults inside
/// `libnvidia-eglcore` tearing down Skia's GL context *at quit* (4/4 runs, vs
/// 0/4 with the renderer off). It costs a coredump on exit and nothing else —
/// saves flush before the window is destroyed, and gameplay survives window
/// resizes at a sustained 60 fps. 12.9 fps is not a shippable alternative.
///
/// **X11 / XWayland.** A different failure with no known workaround: GBM buffer
/// allocation fails (`Failed to create GBM buffer: Invalid argument`) and
/// nothing renders at all, so there the renderer really must be disabled.
///
/// Gated on `/dev/nvidiactl`, created by the nvidia kernel module and absent
/// under nouveau — Mesa/AMD/Intel have neither bug and keep their defaults.
/// `__NV_DISABLE_EXPLICIT_SYNC` is read only by NVIDIA's egl-wayland, so it is
/// inert everywhere else. An explicit value in the environment always wins, per
/// variable, so either decision can be overridden on its own.
///
/// Must run before GTK/WebKit initializes (i.e. before `Builder::run`), and
/// while still single-threaded — `set_var` is only sound before threads spawn.
#[cfg(target_os = "linux")]
fn apply_nvidia_render_fix() {
    const DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    const EXPLICIT_SYNC: &str = "__NV_DISABLE_EXPLICIT_SYNC";

    let gdk_backend = std::env::var("GDK_BACKEND").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland = prefers_wayland(
        gdk_backend.as_deref(),
        wayland_display.as_deref(),
        session_type.as_deref(),
    );

    match decide_nvidia_render_fix(
        std::path::Path::new("/dev/nvidiactl").exists(),
        wayland,
    ) {
        NvidiaRenderFix::None => {}
        NvidiaRenderFix::DisableExplicitSync => {
            if std::env::var_os(EXPLICIT_SYNC).is_none() {
                std::env::set_var(EXPLICIT_SYNC, "1");
            }
        }
        NvidiaRenderFix::DisableDmabufRenderer => {
            if std::env::var_os(DMABUF).is_none() {
                std::env::set_var(DMABUF, "1");
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    apply_nvidia_render_fix();

    let builder = tauri::Builder::default();

    // Desktop-only plugins. ORDER MATTERS: single-instance must be the very
    // first plugin registered so a second launch is intercepted before any
    // other plugin (or the webview) initializes.
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                // Remember size/position/maximized. Deliberately NOT
                // FULLSCREEN — F11 is a transient in-game toggle.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .run(tauri::generate_context!())
        .expect("error while running Red Republic");
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn non_nvidia_keeps_webkit_defaults() {
        assert_eq!(decide_nvidia_render_fix(false, true), NvidiaRenderFix::None);
        assert_eq!(decide_nvidia_render_fix(false, false), NvidiaRenderFix::None);
    }

    #[test]
    fn nvidia_on_wayland_keeps_the_dmabuf_renderer() {
        // The whole point of the fix: Wayland must NOT disable the renderer, or
        // the canvas falls off a cliff to ~13 fps.
        assert_eq!(
            decide_nvidia_render_fix(true, true),
            NvidiaRenderFix::DisableExplicitSync
        );
    }

    #[test]
    fn nvidia_on_x11_still_disables_the_renderer() {
        // X11 fails in GBM allocation, which dropping explicit sync cannot fix.
        assert_eq!(
            decide_nvidia_render_fix(true, false),
            NvidiaRenderFix::DisableDmabufRenderer
        );
    }

    #[test]
    fn explicit_gdk_backend_outranks_the_session() {
        // GDK_BACKEND=x11 inside a Wayland session is XWayland — the X11 path.
        assert!(!prefers_wayland(Some("x11"), Some("wayland-0"), Some("wayland")));
        assert!(prefers_wayland(Some("wayland"), None, None));
        // comma-separated preference list: the first entry is what GTK takes
        assert!(prefers_wayland(Some("wayland,x11"), None, None));
        assert!(!prefers_wayland(Some("x11,wayland"), Some("wayland-0"), None));
    }

    #[test]
    fn session_vars_decide_when_no_backend_is_forced() {
        assert!(prefers_wayland(None, Some("wayland-0"), None));
        assert!(prefers_wayland(None, None, Some("wayland")));
        assert!(!prefers_wayland(None, None, Some("x11")));
        assert!(!prefers_wayland(None, None, None));
        // an empty GDK_BACKEND requests nothing and must not mask the session
        assert!(prefers_wayland(Some(""), Some("wayland-0"), None));
        assert!(!prefers_wayland(None, Some(""), None));
    }
}
