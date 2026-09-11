/**
 * Tauri desktop runtime detection.
 *
 * Tauri v2 injects `window.__TAURI__` (via `withGlobalTauri`/`app.withGlobalTauri`
 * or the `@tauri-apps/api` bundle). When present, the desktop installer commands
 * are available through `window.__TAURI__.core.invoke`; otherwise the web build
 * keeps the legacy copy-to-clipboard behaviour.
 */
export interface TauriWindow extends Window {
  __TAURI__?: {
    core: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>
    }
  }
}

export function getTauriWindow(): TauriWindow | undefined {
  if (typeof window === 'undefined') {
    return undefined
  }
  return window as TauriWindow
}

/** True when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return typeof getTauriWindow()?.__TAURI__?.core?.invoke === 'function'
}

/**
 * Invoke a Tauri command when running in the desktop shell.
 *
 * Returns `null` when not in a Tauri environment so callers can branch to the
 * web fallback without throwing.
 */
export function invokeTauri<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> | null {
  const tauri = getTauriWindow()?.__TAURI__
  if (!tauri?.core?.invoke) {
    return null
  }
  return tauri.core.invoke<T>(cmd, args)
}
