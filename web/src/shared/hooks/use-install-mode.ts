import { useCallback, useState } from 'react'
import {
  DEFAULT_INSTALL_MODE,
  readStoredInstallMode,
  saveInstallMode,
  type InstallMode,
} from '@/shared/lib/install-mode'

/**
 * Reactive view of the install-mode preference, for the settings overlay.
 *
 * Only the overlay needs to re-render on change. Consumers that *act* on the
 * mode (install, attach) should call [`resolveInstallMode`] at click time
 * instead of reading this hook's value, which is captured at mount — see that
 * function for why.
 */
export function useInstallMode() {
  const [mode, setModeState] = useState<InstallMode>(
    () => readStoredInstallMode() ?? DEFAULT_INSTALL_MODE,
  )

  const setMode = useCallback((nextMode: InstallMode) => {
    saveInstallMode(nextMode)
    setModeState(nextMode)
  }, [])

  return { mode, setMode }
}
