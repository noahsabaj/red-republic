// ============================================================
// Desktop (Tauri) shell integration. Every export is a safe no-op in
// the browser: all @tauri-apps imports are dynamic and gated on
// isTauri(), so the web bundle never fetches Tauri code.
//
// Owns: window close-request routing (the confirm-quit seam), the
// window-mode surface (Options + F11), browser-accelerator suppression,
// and the launch update check. Any quit path MUST go through quitApp() —
// it flushes pending storage writes and uses destroy(), which bypasses
// close-requested and therefore cannot loop back into the confirm dialog.
//
// Window mode has ONE owner: this module subscribes to the settings store
// and applies whatever `windowMode` says. Nothing else calls a window API —
// the Options control and F11 both simply write the setting, so the panel
// and the hotkey can never disagree about what the window is doing.
// ============================================================
import { getSettings, subscribeSettings, updateSettings } from '@/app/settings';
import type { WindowMode } from '@/app/settings';
import { flushPending, isTauri } from './storage';

export { isTauri };

// ---------- close-request seam ----------
// App registers a handler deciding what the titlebar X / Alt+F4 does:
//   'quit'    -> flush storage and destroy the window
//   'confirm' -> App has taken over (paused + confirm-quit dialog)
export type CloseDecision = 'quit' | 'confirm';

let closeHandler: (() => CloseDecision) | null = null;

export function setCloseRequestHandler(fn: () => CloseDecision): () => void {
  closeHandler = fn;
  return () => {
    if (closeHandler === fn) closeHandler = null;
  };
}

/** Flush pending writes, then hard-destroy the window. */
export async function quitApp(): Promise<void> {
  try {
    await flushPending();
  } catch {
    // quit regardless — the cache already served every read this session
  }
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow().destroy();
}

// ---------- update seam ----------
export interface PendingUpdate {
  version: string;
  install: () => Promise<void>;
}

let availableUpdate: PendingUpdate | null = null;
let updateListener: ((u: PendingUpdate) => void) | null = null;

/** App subscribes; fires immediately if the launch check already finished. */
export function onUpdateAvailable(cb: (u: PendingUpdate) => void): () => void {
  updateListener = cb;
  if (availableUpdate) cb(availableUpdate);
  return () => {
    if (updateListener === cb) updateListener = null;
  };
}

async function checkForUpdates(): Promise<void> {
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    if (!update) return;
    availableUpdate = {
      version: update.version,
      install: async () => {
        // Flush BEFORE installing: on Windows the installer exits the app
        // at the end of downloadAndInstall(), so nothing after it may run.
        await flushPending();
        await update.downloadAndInstall();
        const { relaunch } = await import('@tauri-apps/plugin-process');
        await relaunch(); // macOS/Linux; Windows already relaunched via installer
      },
    };
    updateListener?.(availableUpdate);
  } catch {
    // offline / rate-limited / no published release yet — retry next launch
  }
}

// ---------- browser-chrome suppression ----------
// The game handles F5/F9 itself (with preventDefault); this list covers the
// accelerators the game does NOT handle. DOM-prevented keys are not processed
// as WebView2 accelerators, so this is a cross-platform, args-free approach.
const PLAIN_SUPPRESS = new Set(['F3', 'F5', 'F7']);
const CTRL_SUPPRESS = new Set(['f', 'g', 'h', 'j', 'o', 'p', 'r', 's', 'u', '+', '-', '=', '0']);

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'F11') {
    e.preventDefault();
    // Write the setting, not the window — the subscription applies it, so the
    // Options panel always shows what the hotkey just did.
    updateSettings({ windowMode: fullscreenToggleTarget(getSettings().windowMode, lastFramedMode) });
    return;
  }
  if (!import.meta.env.DEV && e.key === 'F12') {
    e.preventDefault();
    return;
  }
  if (PLAIN_SUPPRESS.has(e.key)) {
    e.preventDefault();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && !e.altKey && CTRL_SUPPRESS.has(e.key.toLowerCase())) {
    e.preventDefault();
    return;
  }
  if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) e.preventDefault();
}

// ---------- window mode ----------

/**
 * F11's destination: into fullscreen, or back out to the framing that preceded
 * it. Pure so the rule is testable without a window — `framed` is the last
 * non-fullscreen mode the player was in, so a borderless player who taps F11
 * twice lands back in borderless rather than being dumped into a title bar.
 */
export function fullscreenToggleTarget(current: WindowMode, framed: WindowMode): WindowMode {
  return current === 'fullscreen' ? framed : 'fullscreen';
}

// Session-only: where F11 returns to. Deliberately not persisted — the saved
// setting IS the player's choice, and this only answers "and back to what?".
let lastFramedMode: WindowMode = 'windowed';
let appliedMode: WindowMode | null = null;

/**
 * Drive the real window to `mode`. Safe no-op in the browser.
 *
 * `borderless` is decorations-off plus maximize rather than explicit
 * work-area geometry: the OS already knows where the taskbar is, gets it
 * right on whichever monitor the window is on, and needs no scale-factor
 * math — so this asks for two window permissions instead of four.
 */
async function applyWindowMode(mode: WindowMode): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const win = getCurrentWindow();
  // Leave fullscreen before reframing — decoration and maximize changes are
  // ignored, or land on the wrong geometry, while the window owns the display.
  if (mode !== 'fullscreen') await win.setFullscreen(false);
  switch (mode) {
    case 'fullscreen':
      await win.setFullscreen(true);
      break;
    case 'borderless':
      await win.setDecorations(false);
      await win.maximize();
      break;
    case 'windowed':
      await win.setDecorations(true);
      await win.unmaximize();
      break;
  }
}

/** Apply `mode` if it isn't already applied, remembering F11's way back. */
function syncWindowMode(mode: WindowMode): void {
  if (mode === appliedMode) return;
  appliedMode = mode;
  if (mode !== 'fullscreen') lastFramedMode = mode;
  void applyWindowMode(mode);
}

// ---------- init ----------

/** Called by main.tsx after initStorage(), before the first render. */
export async function initDesktop(): Promise<void> {
  if (!isTauri()) return;

  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow().onCloseRequested(event => {
    // Prevent synchronously, then decide — async work after the handler
    // returns would race the close. quitApp() destroys, so the granted
    // path never re-enters this handler.
    event.preventDefault();
    if ((closeHandler?.() ?? 'quit') === 'quit') void quitApp();
  });

  // The window boots fullscreen from tauri.conf, so a player on the default
  // never sees a transition; anyone who chose otherwise is corrected here,
  // before the first render. Then this module owns the mode for the session.
  syncWindowMode(getSettings().windowMode);
  subscribeSettings(() => syncWindowMode(getSettings().windowMode));

  window.addEventListener('keydown', onKeydown);
  // Ctrl+wheel would zoom the entire HUD
  window.addEventListener('wheel', e => { if (e.ctrlKey) e.preventDefault(); }, { passive: false });
  // right-click is a game input; keep inspect-element in dev builds
  if (!import.meta.env.DEV) window.addEventListener('contextmenu', e => e.preventDefault());

  if (!import.meta.env.DEV) void checkForUpdates();
}
