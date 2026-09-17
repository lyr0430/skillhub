/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { INSTALL_MODE_STORAGE_KEY } from '@/shared/lib/install-mode'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

import { SettingsDialog } from './settings-dialog'

describe('SettingsDialog', () => {
  // Auto-cleanup is not enabled in this project's vitest config.
  afterEach(() => cleanup())

  beforeEach(() => {
    window.localStorage.clear()
  })

  const renderOpen = (onOpenChange = vi.fn()) => {
    render(<SettingsDialog open onOpenChange={onOpenChange} />)
    return onOpenChange
  }

  it('shows the left menu and both install modes', () => {
    renderOpen()

    expect(screen.getByTestId('settings-menu-skill')).toBeTruthy()
    // Both modes are described up front, so the difference is readable without
    // selecting anything.
    expect(screen.getByTestId('install-mode-option-shared')).toBeTruthy()
    expect(screen.getByTestId('install-mode-option-copy')).toBeTruthy()
  })

  it('exposes the modes as a single-choice radio group', () => {
    renderOpen()

    const group = screen.getByRole('radiogroup')
    expect(group).toBeTruthy()
    const radios = screen.getAllByRole('radio')
    expect(radios).toHaveLength(2)
  })

  it('defaults to shared install', () => {
    renderOpen()

    expect(
      screen.getByTestId('install-mode-option-shared').getAttribute('aria-checked'),
    ).toBe('true')
    expect(
      screen.getByTestId('install-mode-option-copy').getAttribute('aria-checked'),
    ).toBe('false')
  })

  it('switches to the independent copy mode and persists it', () => {
    renderOpen()

    fireEvent.click(screen.getByTestId('install-mode-option-copy'))

    expect(
      screen.getByTestId('install-mode-option-copy').getAttribute('aria-checked'),
    ).toBe('true')
    expect(
      screen.getByTestId('install-mode-option-shared').getAttribute('aria-checked'),
    ).toBe('false')
    expect(window.localStorage.getItem(INSTALL_MODE_STORAGE_KEY)).toBe('copy')
  })

  it('reads the stored preference when opened', () => {
    window.localStorage.setItem(INSTALL_MODE_STORAGE_KEY, 'copy')
    renderOpen()

    expect(
      screen.getByTestId('install-mode-option-copy').getAttribute('aria-checked'),
    ).toBe('true')
  })

  it('marks the active menu item for assistive technology', () => {
    // The visual weight was too subtle to rely on alone, so the selected state is
    // also exposed through aria-current.
    renderOpen()

    expect(screen.getByTestId('settings-menu-skill').getAttribute('aria-current')).toBe('page')
  })

  it('renders nothing while closed', () => {
    render(<SettingsDialog open={false} onOpenChange={vi.fn()} />)

    expect(screen.queryByTestId('settings-menu-skill')).toBeNull()
  })

  it('asks to close on Escape', () => {
    const onOpenChange = renderOpen()

    fireEvent.keyDown(document, { key: 'Escape' })

    expect(onOpenChange).toHaveBeenCalledWith(false)
  })
})
