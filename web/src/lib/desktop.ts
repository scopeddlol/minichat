/**
 * Bridge to the MiniChat desktop shell.
 *
 * The desktop app is a frameless Tauri window pointed at this instance, which
 * means the titlebar and its buttons are ours to draw — see `<TitleBar />`.
 * Tauri injects its JS API into every page the webview loads, so no npm
 * dependency is needed here, but each command still has to be granted: the
 * shell grants this origin a capability covering exactly the window
 * controls below and nothing else (`grant_instance_ipc` in desktop/src/main.rs).
 *
 * Every call is therefore best-effort. In a browser the bridge is simply
 * absent, and `<TitleBar />` never renders.
 */

interface TauriWindow {
  minimize(): Promise<void>
  toggleMaximize(): Promise<void>
  close(): Promise<void>
  isMaximized(): Promise<boolean>
}

declare global {
  interface Window {
    __TAURI__?: {
      core?: { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> }
      window?: { getCurrentWindow?: () => TauriWindow }
    }
  }
}

/**
 * Whether this page is running inside the desktop app.
 *
 * Keyed on the user agent the shell sets, not on the presence of the Tauri
 * globals: the UA is what the shell controls deliberately, and it is the same
 * signal the settings screen uses to decide whether to offer a download.
 */
export const isDesktopApp = /\bMiniChat\/[\d.]+ Desktop\b/.test(navigator.userAgent)

/**
 * The window handle, or null if the bridge isn't there.
 *
 * Resolved per call rather than captured once, so a slow injection can't
 * leave the titlebar permanently dead.
 */
function handle(): TauriWindow | null {
  try {
    return window.__TAURI__?.window?.getCurrentWindow?.() ?? null
  } catch {
    return null
  }
}

/** True once the window controls can actually be driven. */
export function hasWindowControls(): boolean {
  return isDesktopApp && handle() !== null
}

async function run<T>(action: (win: TauriWindow) => Promise<T>): Promise<T | null> {
  const win = handle()
  if (!win) return null
  try {
    return await action(win)
  } catch (error) {
    // A denied command means the capability grant and this file have drifted
    // apart. Worth seeing in the console; never worth breaking the app over.
    console.warn('minichat: window control unavailable', error)
    return null
  }
}

export const windowControls = {
  minimize: () => run((win) => win.minimize()),
  toggleMaximize: () => run((win) => win.toggleMaximize()),
  close: () => run((win) => win.close()),
  isMaximized: () => run((win) => win.isMaximized()),
}
