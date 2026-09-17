/**
 * How a skill is installed onto a local agent.
 *
 * - `shared` — the real files live once in `~/.skillhub/skills/<slug>/` and each
 *   agent gets a symlink to them. Updating once updates every agent.
 * - `copy`   — each agent gets its own independent copy (the historical default).
 *
 * The preference is a client-side UI setting, stored like the theme: it is never
 * sent to the registry and never tied to an account. It only affects *future*
 * installs; switching it must not touch anything already installed.
 */
export type InstallMode = 'shared' | 'copy'

/** Mirrors `THEME_STORAGE_KEY`'s `skillhub-` prefix so local keys stay uniform. */
export const INSTALL_MODE_STORAGE_KEY = 'skillhub-install-mode'

/** Shared install is the default: it is what makes one skill serve many agents. */
export const DEFAULT_INSTALL_MODE: InstallMode = 'shared'

interface ModeStorage {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
}

export function isInstallMode(value: unknown): value is InstallMode {
  return value === 'shared' || value === 'copy'
}

/**
 * Read the stored preference, or `null` when unset or unreadable.
 *
 * An unrecognised value reads as `null` rather than throwing: a stale or
 * hand-edited key must not break the settings overlay, and the caller falls back
 * to [`DEFAULT_INSTALL_MODE`].
 */
export function readStoredInstallMode(
  storage: ModeStorage | undefined = getBrowserStorage(),
): InstallMode | null {
  if (!storage) return null

  try {
    const value = storage.getItem(INSTALL_MODE_STORAGE_KEY)
    return isInstallMode(value) ? value : null
  } catch {
    return null
  }
}

/** Persist the preference. Storage failures are non-fatal (private browsing). */
export function saveInstallMode(
  mode: InstallMode,
  storage: ModeStorage | undefined = getBrowserStorage(),
) {
  if (!storage) return

  try {
    storage.setItem(INSTALL_MODE_STORAGE_KEY, mode)
  } catch {
    // Storage can be unavailable in private browsing or hardened browser contexts.
  }
}

/**
 * The effective mode right now: the stored preference, else the default.
 *
 * Action handlers (install, attach) must call this at click time rather than
 * closing over a hook's state. The overlay and the install button are separate
 * components that never re-render each other, so a mode captured in `useState`
 * would be the value from mount — and a user who switches the mode and then
 * immediately installs would get the previous mode.
 */
export function resolveInstallMode(
  storage: ModeStorage | undefined = getBrowserStorage(),
): InstallMode {
  return readStoredInstallMode(storage) ?? DEFAULT_INSTALL_MODE
}

function getBrowserStorage(): Storage | undefined {
  if (typeof window === 'undefined') return undefined

  try {
    return window.localStorage
  } catch {
    return undefined
  }
}
