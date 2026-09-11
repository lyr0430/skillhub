// @vitest-environment jsdom

import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { InstallForAgentButton, buildAgentInstallPrompt } from './install-for-agent-button'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) => key === 'skillDetail.installForAgent.prompt'
      ? `Connect with ${values?.guideUrl}; install ${values?.skill} version ${values?.version}.`
      : key,
  }),
}))

const hoisted = vi.hoisted(() => {
  const state = { isTauri: false }
  const agents = [
    { id: 'claude-code', name: 'Claude Code', dir: '/home/u/.claude/skills', installed: true },
    { id: 'generic', name: '默认全局 Skill 位置', dir: '/home/u/.agents/skills', installed: false },
  ]
  return { state, agents }
})

// Tauri runtime detection + installer are mocked so the desktop path is testable.
vi.mock('@/shared/lib/tauri', () => ({
  isTauri: () => hoisted.state.isTauri,
  invokeTauri: (cmd: string, _args?: Record<string, unknown>) => {
    if (cmd === 'detect_agents') {
      return Promise.resolve({ ok: true, data: hoisted.agents, error: undefined })
    }
    if (cmd === 'install_skill_command') {
      return Promise.resolve({
        ok: true,
        data: { ok: true, dir: '/home/u/.claude/skills/my-skill', agent: 'claude-code', warnings: [] },
        error: undefined,
      })
    }
    return Promise.resolve(null)
  },
}))

describe('install-for-agent-button', () => {
  const originalRuntimeConfig = window.__SKILLHUB_RUNTIME_CONFIG__

  const formatPrompt = (guideUrl: string, skill: string, version: string) => (
    `Connect with ${guideUrl}; install ${skill} version ${version}.`
  )

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
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
      expect(getByTestId('install-confirm')).toBeTruthy()
    })

    it('invokes the desktop installer for the selected agent', async () => {
      const { getByTestId } = render(createElement(InstallForAgentButton, {
        namespace: 'global',
        slug: 'my-skill',
        version: '1.2.3',
      }))

      await act(async () => fireEvent.click(getByTestId('install-for-agent-button')))
      await waitFor(() => expect(getByTestId('install-target-claude-code')).toBeTruthy())

      await act(async () => fireEvent.click(getByTestId('install-confirm')))

      // Success state shows the installed directory path (dialog stays open).
      await waitFor(() => {
        expect(document.body.textContent).toContain('/home/u/.claude/skills/my-skill')
      })
    })
  })
})
