/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const hoisted = vi.hoisted(() => ({ state: { isTauri: false } }))

vi.mock('@/shared/lib/tauri', () => ({
  isTauri: () => hoisted.state.isTauri,
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

import { SettingsTrigger } from './settings-trigger'

describe('SettingsTrigger', () => {
  afterEach(() => cleanup())

  beforeEach(() => {
    hoisted.state.isTauri = false
  })

  it('renders nothing in the browser', () => {
    const { container } = render(<SettingsTrigger open={false} onToggle={() => {}} />)

    expect(container.innerHTML).toBe('')
    expect(screen.queryByTestId('settings-trigger')).toBeNull()
  })

  it('shows the gear in the desktop shell', () => {
    hoisted.state.isTauri = true
    render(<SettingsTrigger open={false} onToggle={() => {}} />)

    expect(screen.getByTestId('settings-trigger')).toBeTruthy()
  })

  it('reports its expanded state', () => {
    hoisted.state.isTauri = true
    const { rerender } = render(<SettingsTrigger open={false} onToggle={() => {}} />)
    expect(screen.getByTestId('settings-trigger').getAttribute('aria-expanded')).toBe('false')

    rerender(<SettingsTrigger open onToggle={() => {}} />)
    expect(screen.getByTestId('settings-trigger').getAttribute('aria-expanded')).toBe('true')
  })

  it('calls onToggle when clicked', () => {
    hoisted.state.isTauri = true
    const onToggle = vi.fn()
    render(<SettingsTrigger open={false} onToggle={onToggle} />)

    fireEvent.click(screen.getByTestId('settings-trigger'))

    expect(onToggle).toHaveBeenCalledTimes(1)
  })
})
