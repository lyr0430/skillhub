// @vitest-environment jsdom

import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { InstallForAgentButton, buildAgentInstallPrompt } from './install-for-agent-button'

// Stable `t` handler so it stays referentially equal across renders (real
// react-i18next returns a stable `t`; an inline arrow would loop the effect dep).
const mockT = (key: string, values?: Record<string, string>) =>
  key === 'skillDetail.installForAgent.prompt'
    ? `Connect with ${values?.guideUrl}; install ${values?.skill} version ${values?.version}.`
    : key

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: mockT }),
}))

const hoisted = vi.hoisted(() => {
  const state = { isTauri: false }
  const agents = [
    { id: 'claude-code', name: 'Claude Code', dir: '/home/u/.claude/skills', installed: true },
    { id: 'generic', name: '默认全局 Skill 位置', dir: '/home/u/.agents/skills', installed: false },
  ]
  // Per-agent install status returned by detect_skill_status. claude-code is
  // already installed (with an outdated version) so the update path is testable;
  // generic has a same-named non-skillhub dir (unmanaged) so the conflict prompt
  // is testable.
  const statuses = [
    { agent: 'claude-code', installed: true, version: '1.0.0', outdated: true, unmanaged: false },
    { agent: 'generic', installed: false, version: '', outdated: false, unmanaged: true },
  ]
  const toastSuccess = vi.fn()
  const toastError = vi.fn()
  const installArgs: Array<Record<string, unknown>> = []
  return { state, agents, statuses, toastSuccess, toastError, installArgs }
})

// Tauri runtime detection + installer are mocked so the desktop path is testable.
vi.mock('@/shared/lib/tauri', () => ({
  isTauri: () => hoisted.state.isTauri,
  invokeTauri: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === 'detect_agents') {
      return Promise.resolve({ ok: true, data: hoisted.agents, error: undefined })
    }
    if (cmd === 'detect_skill_status') {
      return Promise.resolve({ ok: true, data: hoisted.statuses, error: undefined })
    }
    if (cmd === 'install_skill_command') {
      hoisted.installArgs.push(args ?? {})
      return Promise.resolve({
        ok: true,
        data: { ok: true, dir: '/home/u/.claude/skills/my-skill', agent: 'claude-code', warnings: [] },
        error: undefined,
      })
    }
    if (cmd === 'uninstall_skill_command') {
      return Promise.resolve({
        ok: true,
        data: { ok: true, dir: '/home/u/.claude/skills/my-skill', agent: args?.agent },
        error: undefined,
      })
    }
    return Promise.resolve(null)
  },
}))

// Toast is a no-op in tests; success/error are spied via hoisted.
vi.mock('@/shared/lib/toast', () => ({
  toast: {
    success: hoisted.toastSuccess,
    error: hoisted.toastError,
    warning: vi.fn(),
    info: vi.fn(),
    promise: vi.fn(),
  },
  centeredToastOptions: vi.fn(),
  CENTER_TOASTER_ID: 'top-center',
}))

describe('install-for-agent-button', () => {
  const originalRuntimeConfig = window.__SKILLHUB_RUNTIME_CONFIG__

  const formatPrompt = (guideUrl: string, skill: string, version: string) => (
    `Connect with ${guideUrl}; install ${skill} version ${version}.`
  )

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
    vi.clearAllMocks()
    hoisted.toastSuccess.mockClear()
    hoisted.toastError.mockClear()
    hoisted.installArgs.length = 0
    hoisted.state.isTauri = false
    window.__SKILLHUB_RUNTIME_CONFIG__ = originalRuntimeConfig
  })

  it('builds a prompt for a global skill using the instance guide', () => {
    expect(buildAgentInstallPrompt('global', 'my-skill', '1.2.3', 'https://skill.example.com', formatPrompt)).toBe(
      'Connect with https://skill.example.com/registry/skill.md; install @global/my-skill version 1.2.3.',
    )
  })

  it('keeps a sub-path base and namespace in the copied prompt', () => {
    expect(buildAgentInstallPrompt('team-alpha', 'my-skill', '2.0.0', 'https://skill.example.com/skillhub/', formatPrompt)).toBe(
      'Connect with https://skill.example.com/skillhub/registry/skill.md; install @team-alpha/my-skill version 2.0.0.',
    )
  })

  it('renders an accessible copy button', () => {
    const html = renderToStaticMarkup(createElement(InstallForAgentButton, {
      namespace: 'global',
      slug: 'my-skill',
      version: '1.2.3',
    }))

    expect(html).toContain('data-testid="install-for-agent-button"')
    expect(html).toContain('aria-label="skillDetail.installForAgent.button"')
    expect(html).toContain('skillDetail.installForAgent.button')
  })

  it('can be disabled when the selected skill version is not installable', () => {
    const html = renderToStaticMarkup(createElement(InstallForAgentButton, {
      namespace: 'global',
      slug: 'my-skill',
      version: '1.2.3',
      disabled: true,
    }))

    expect(html).toContain('disabled=""')
  })

  it('is disabled for a version that cannot be copied safely across shells', () => {
    const html = renderToStaticMarkup(createElement(InstallForAgentButton, {
      namespace: 'global',
      slug: 'my-skill',
      version: '1.0.0&echo INJECTED',
    }))

    expect(html).toContain('disabled=""')
  })

  it('copies the complete instance, coordinate, and version prompt', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    })
    window.__SKILLHUB_RUNTIME_CONFIG__ = { appBaseUrl: 'https://skill.example.com/skillhub' }

    const { getByTestId } = render(createElement(InstallForAgentButton, {
      namespace: 'team-alpha',
      slug: 'my-skill',
      version: '2.0.0',
    }))

    await act(async () => fireEvent.click(getByTestId('install-for-agent-button')))

    await waitFor(() => expect(writeText).toHaveBeenCalledWith(
      'Connect with https://skill.example.com/skillhub/registry/skill.md; install @team-alpha/my-skill version 2.0.0.',
    ))
  })

  describe('desktop (Tauri) path', () => {
    beforeEach(() => {
      hoisted.state.isTauri = true
    })

    it('renders an install button that opens the target dialog', async () => {
      const { getByTestId } = render(createElement(InstallForAgentButton, {
        namespace: 'global',
        slug: 'my-skill',
        version: '1.2.3',
      }))

      await act(async () => fireEvent.click(getByTestId('install-for-agent-button')))

      // Dialog opens and enlists the detected agents.
      await waitFor(() => {
        expect(getByTestId('install-target-claude-code')).toBeTruthy()
        expect(getByTestId('install-target-generic')).toBeTruthy()
      })
      // A pre-installed agent exposes uninstall + update.
      expect(getByTestId('install-uninstall')).toBeTruthy()
      expect(getByTestId('install-update')).toBeTruthy()
    })

    it('invokes the desktop installer and shows a toast for the selected agent', async () => {
      const { getByTestId } = render(createElement(InstallForAgentButton, {
        namespace: 'global',
        slug: 'my-skill',
        version: '1.2.3',
      }))

      await act(async () => fireEvent.click(getByTestId('install-for-agent-button')))
      await waitFor(() => expect(getByTestId('install-target-claude-code')).toBeTruthy())

      // claude-code is installed & outdated → "更新" button triggers the install.
      await act(async () => fireEvent.click(getByTestId('install-update')))

      // Install succeeds → toast success and the dialog closes.
      await waitFor(() => {
        expect(hoisted.toastSuccess).toHaveBeenCalled()
      })
    })

    it('uninstalls the selected agent after a confirmation dialog', async () => {
      const { getByTestId, queryByTestId } = render(createElement(InstallForAgentButton, {
        namespace: 'global',
        slug: 'my-skill',
        version: '1.2.3',
      }))

      await act(async () => fireEvent.click(getByTestId('install-for-agent-button')))
      await waitFor(() => expect(getByTestId('install-uninstall')).toBeTruthy())

      // Clicking uninstall opens the confirmation dialog, not an immediate uninstall.
      await act(async () => fireEvent.click(getByTestId('install-uninstall')))
      await waitFor(() => expect(getByTestId('confirm-install-uninstall')).toBeTruthy())

      // Confirming performs the actual uninstall and shows a success toast.
      await act(async () => fireEvent.click(getByTestId('confirm-install-uninstall')))

      await waitFor(() => {
        expect(hoisted.toastSuccess).toHaveBeenCalled()
      })
      expect(queryByTestId('confirm-install-uninstall')).toBeNull()
    })

    it('asks to refactor or back up before installing over a non-skillhub dir', async () => {
      const { getByTestId } = render(createElement(InstallForAgentButton, {
        namespace: 'global',
        slug: 'my-skill',
        version: '1.2.3',
      }))

      await act(async () => fireEvent.click(getByTestId('install-for-agent-button')))
      await waitFor(() => expect(getByTestId('install-target-generic')).toBeTruthy())

      // generic is unmanaged (same-named non-skillhub dir) so install prompts.
      await act(async () => fireEvent.click(getByTestId('install-target-generic')))
      await act(async () => fireEvent.click(getByTestId('install-confirm')))

      // The conflict dialog appears with overwrite/backup choices.
      await waitFor(() => expect(getByTestId('install-backup')).toBeTruthy())
      expect(getByTestId('install-overwrite')).toBeTruthy()

      // Choosing backup installs with preserveExisting=true.
      await act(async () => fireEvent.click(getByTestId('install-backup')))

      await waitFor(() => {
        expect(hoisted.installArgs.some((a) => {
          const input = a.input as { agent?: string; preserveExisting?: boolean }
          return input?.agent === 'generic' && input.preserveExisting === true
        })).toBe(true)
        expect(hoisted.toastSuccess).toHaveBeenCalled()
      })
    })
  })
})
