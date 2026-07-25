#[cfg(desktop)]
use tauri::Manager;

/// WebKitGTK's DMA-BUF renderer segfaults inside NVIDIA's proprietary EGL
/// driver when a GL context is torn down (`WebCore::GLContext::~GLContext` ->
/// libEGL_nvidia -> libnvidia-eglcore), killing the web process the instant
/// the window opens — the window appears and vanishes with no error on stdout.
/// Disabling that renderer is the upstream workaround.
///
/// Gated on the proprietary driver specifically: `/dev/nvidiactl` is created by
/// the nvidia kernel module and is absent under nouveau, which renders through
/// Mesa and does not have the bug. Everyone else keeps accelerated compositing,
/// which a canvas-heavy renderer wants. An explicit value in the environment
/// always wins, so `WEBKIT_DISABLE_DMABUF_RENDERER=0` forces it back on.
///
/// Must run before GTK/WebKit initializes (i.e. before `Builder::run`), and
/// while still single-threaded — `set_var` is only sound before threads spawn.
#[cfg(target_os = "linux")]
fn disable_dmabuf_renderer_on_nvidia() {
    const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(VAR).is_none() && std::path::Path::new("/dev/nvidiactl").exists() {
        std::env::set_var(VAR, "1");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    disable_dmabuf_renderer_on_nvidia();

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
