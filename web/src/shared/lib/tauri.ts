import { invoke } from '@tauri-apps/api/core'

/**
 * Tauri desktop runtime detection.
 *
 * Tauri v2 always injects `window.__TAURI_INTERNALS__` inside the desktop
 * WebView, so we detect on that (more reliable than `window.__TAURI__`, which
 * only appears when `app.withGlobalTauri` is enabled). In a plain browser the
 * global is absent, and the web build keeps the legacy copy-to-clipboard
 * behaviour.
 */
function isTauriRuntime(): boolean {
  if (typeof window === 'undefined') {
    return false
  }
  return typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== 'undefined'
}

/** True when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return isTauriRuntime()
}

/**
 * Invoke a Tauri command when running in the desktop shell.
 *
 * Uses the official `@tauri-apps/api` `invoke`, which routes through
 * `window.__TAURI_INTERNALS__.invoke`. Returns `null` when not in a Tauri
 * environment so callers can branch to the web fallback without throwing.
 */
export function invokeTauri<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> | null {
  if (!isTauriRuntime()) {
    return null
  }
  return invoke<T>(cmd, args)
}
