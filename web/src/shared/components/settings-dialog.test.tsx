/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { INSTALL_MODE_STORAGE_KEY } from '@/shared/lib/install-mode'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

vi.mock('@/features/skill/tauri-installer', () => ({
  getSkillStoragePath: vi.fn(),
  setSkillStoragePath: vi.fn(),
  resetSkillStoragePath: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

import { getSkillStoragePath, resetSkillStoragePath, setSkillStoragePath } from '@/features/skill/tauri-installer'
import { open as openDialog } from '@tauri-apps/plugin-dialog'

import { SettingsDialog } from './settings-dialog'

describe('SettingsDialog', () => {
  // Auto-cleanup is not enabled in this project's vitest config.
  afterEach(() => cleanup())

  beforeEach(() => {
    window.localStorage.clear()
    vi.mocked(getSkillStoragePath).mockResolvedValue('/default/skillhub/skills')
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

describe('SettingsDialog storage path', () => {
  afterEach(() => cleanup())

  beforeEach(() => {
    window.localStorage.clear()
    vi.mocked(getSkillStoragePath).mockResolvedValue('/default/skillhub/skills')
    vi.mocked(openDialog).mockReset()
    vi.mocked(setSkillStoragePath).mockReset()
    vi.mocked(resetSkillStoragePath).mockReset()
  })

  const renderOpen = () => render(<SettingsDialog open onOpenChange={vi.fn()} />)
  const pathValue = () =>
    (screen.getByTestId('skill-storage-path-input') as HTMLInputElement).value

  it('shows the current storage path once loaded', async () => {
    renderOpen()

    await waitFor(() => expect(pathValue()).toBe('/default/skillhub/skills'))
  })

  it('shows a confirm dialog before choosing a directory', async () => {
    renderOpen()
    await waitFor(() => expect(pathValue()).toBe('/default/skillhub/skills'))

    fireEvent.click(screen.getByTestId('skill-storage-path-pick'))

    expect(screen.getByTestId('storage-confirm-ok')).toBeTruthy()
    expect(openDialog).not.toHaveBeenCalled()
  })

  it('opens the directory picker and migrates after confirm', async () => {
    vi.mocked(openDialog).mockResolvedValue('/new/repo')
    vi.mocked(setSkillStoragePath).mockResolvedValue({
      ok: true,
      newPath: '/new/repo',
      movedSlugs: ['alpha'],
      updatedLinks: ['/tmp/agent/alpha'],
      warnings: [],
    })
    renderOpen()
    await waitFor(() => expect(pathValue()).toBe('/default/skillhub/skills'))

    fireEvent.click(screen.getByTestId('skill-storage-path-pick'))
    fireEvent.click(screen.getByTestId('storage-confirm-ok'))

    await waitFor(() => expect(openDialog).toHaveBeenCalledWith({ directory: true }))
    await waitFor(() => expect(setSkillStoragePath).toHaveBeenCalledWith('/new/repo'))
    await waitFor(() => expect(pathValue()).toBe('/new/repo'))
  })

  it('does nothing when the directory picker is cancelled', async () => {
    vi.mocked(openDialog).mockResolvedValue(null)
    renderOpen()
    await waitFor(() => expect(pathValue()).toBe('/default/skillhub/skills'))

    fireEvent.click(screen.getByTestId('skill-storage-path-pick'))
    fireEvent.click(screen.getByTestId('storage-confirm-ok'))

    await waitFor(() => expect(openDialog).toHaveBeenCalled())
    expect(setSkillStoragePath).not.toHaveBeenCalled()
    expect(pathValue()).toBe('/default/skillhub/skills')
  })

  it('restores the default path after confirm', async () => {
    vi.mocked(getSkillStoragePath).mockResolvedValue('/custom/repo')
    vi.mocked(resetSkillStoragePath).mockResolvedValue({
      ok: true,
      newPath: '/default/skillhub/skills',
      movedSlugs: ['alpha'],
      updatedLinks: ['/tmp/agent/alpha'],
      warnings: [],
    })
    renderOpen()
    await waitFor(() => expect(pathValue()).toBe('/custom/repo'))

    fireEvent.click(screen.getByTestId('skill-storage-path-reset'))
    fireEvent.click(screen.getByTestId('storage-confirm-ok'))

    await waitFor(() => expect(resetSkillStoragePath).toHaveBeenCalled())
    await waitFor(() => expect(pathValue()).toBe('/default/skillhub/skills'))
  })

  it('does not migrate on cancel', async () => {
    renderOpen()
    await waitFor(() => expect(pathValue()).toBe('/default/skillhub/skills'))

    fireEvent.click(screen.getByTestId('skill-storage-path-pick'))
    fireEvent.click(screen.getByTestId('storage-confirm-cancel'))

    expect(openDialog).not.toHaveBeenCalled()
    expect(setSkillStoragePath).not.toHaveBeenCalled()
  })
})
