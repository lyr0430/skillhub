// @vitest-environment jsdom

import { createElement } from 'react'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { LocalSkillsPage } from './local-skills'

// Stable `t` handler so it stays referentially equal across renders (real
// react-i18next returns a stable `t`; an inline arrow would re-trigger the
// refresh effect on every render and leave the list stuck loading).
const stableT = (key: string, values?: Record<string, string>) =>
  key === 'localSkills.agent'
    ? String(values?.agent)
    : key === 'localSkills.version'
      ? `v${values?.version}`
      : key

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: stableT }),
}))

const hoisted = vi.hoisted(() => {
  const defaultSkills = [
    {
      registry: 'https://skill.example.com',
      namespace: 'global',
      slug: 'video-frames',
      version: '1.0.0',
      agent: 'claude-code',
      dir: '/home/u/.claude/skills/video-frames',
    },
  ]
  const state = { isTauri: false, skills: defaultSkills }
  const toastSuccess = vi.fn()
  const toastError = vi.fn()
  const listInstalledSkills = vi.fn().mockImplementation(() => Promise.resolve(hoisted.state.skills))
  const installSkill = vi.fn().mockResolvedValue({ ok: true, dir: '/home/u/.claude/skills/video-frames', agent: 'claude-code', warnings: [] })
  const resolveSkillVersion = vi.fn().mockResolvedValue({ namespace: 'global', slug: 'video-frames', version: '2.0.0' })
  const openInFileManager = vi.fn().mockResolvedValue(undefined)
  return { state, defaultSkills, toastSuccess, toastError, listInstalledSkills, installSkill, resolveSkillVersion, openInFileManager }
})

// High-level API is mocked so the page logic is testable without the runtime bridge.
vi.mock('@/features/skill/tauri-installer', () => ({
  detectAgents: vi.fn().mockResolvedValue([
    { id: 'claude-code', name: 'Claude Code', dir: '/home/u/.claude/skills', installed: true },
    { id: 'generic', name: '默认全局 Skill 位置', dir: '/home/u/.agents/skills', installed: false },
  ]),
  listInstalledSkills: hoisted.listInstalledSkills,
  uninstallSkill: vi.fn().mockResolvedValue({ ok: true, agent: 'claude-code', dir: '/x' }),
  installSkill: hoisted.installSkill,
  openInFileManager: hoisted.openInFileManager,
}))

vi.mock('@/api/client', () => ({
  resolveSkillVersion: hoisted.resolveSkillVersion,
}))

vi.mock('@/shared/lib/tauri', () => ({
  isTauri: () => hoisted.state.isTauri,
  invokeTauri: vi.fn(),
}))

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

vi.mock('@/shared/components/dashboard-page-header', () => ({
  DashboardPageHeader: ({ title }: { title: string }) => <h1>{title}</h1>,
}))

vi.mock('@/features/skill/install-command', () => ({
  getBaseUrl: () => 'https://skill.example.com',
}))

describe('local-skills', () => {
  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
    hoisted.state.isTauri = false
    hoisted.state.skills = hoisted.defaultSkills
  })

  it('renders a desktop-only notice in the browser', () => {
    hoisted.state.isTauri = false
    const { getByText } = render(createElement(LocalSkillsPage))
    expect(getByText('localSkills.desktopOnly')).toBeTruthy()
  })

  it('lists installed local skills', async () => {
    hoisted.state.isTauri = true
    const { getByText } = render(createElement(LocalSkillsPage))

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    expect(getByText('/home/u/.claude/skills/video-frames')).toBeTruthy()
  })

  it('opens a skill folder in the system file manager', async () => {
    hoisted.state.isTauri = true
    const { getByTestId } = render(createElement(LocalSkillsPage))

    await waitFor(() => expect(getByTestId('local-open-claude-code-video-frames')).toBeTruthy())

    fireEvent.click(getByTestId('local-open-claude-code-video-frames'))

    await waitFor(() => {
      expect(hoisted.openInFileManager).toHaveBeenCalledWith('/home/u/.claude/skills/video-frames')
    })
  })

  it('updates to the latest version when a newer one exists', async () => {
    hoisted.state.isTauri = true
    const { getByTestId, getByText } = render(createElement(LocalSkillsPage))

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    hoisted.resolveSkillVersion.mockResolvedValueOnce({
      namespace: 'global',
      slug: 'video-frames',
      version: '2.0.0',
    })

    fireEvent.click(getByTestId('local-update-claude-code-video-frames'))

    await waitFor(() => {
      expect(hoisted.installSkill).toHaveBeenCalledWith(
        {
          namespace: 'global',
          slug: 'video-frames',
          version: '2.0.0',
          agent: 'claude-code',
        },
        'https://skill.example.com',
      )
    })
  })

  it('filters the list by search query', async () => {
    hoisted.state.isTauri = true
    hoisted.state.skills = [
      { registry: 'https://skill.example.com', namespace: 'global', slug: 'alpha', version: '1.0.0', agent: 'claude-code', dir: '/home/u/.claude/skills/alpha' },
      { registry: 'https://skill.example.com', namespace: 'global', slug: 'beta', version: '1.0.0', agent: 'claude-code', dir: '/home/u/.claude/skills/beta' },
    ]
    const { getByPlaceholderText, getByText, queryByText } = render(createElement(LocalSkillsPage))

    await waitFor(() => expect(getByText('@global/alpha')).toBeTruthy())
    expect(getByText('@global/beta')).toBeTruthy()

    fireEvent.change(getByPlaceholderText('localSkills.searchPlaceholder'), {
      target: { value: 'alpha' },
    })

    expect(getByText('@global/alpha')).toBeTruthy()
    expect(queryByText('@global/beta')).toBeNull()
  })

  it('paginates the list beyond the page size', async () => {
    hoisted.state.isTauri = true
    const many = Array.from({ length: 10 }, (_, i) => ({
      registry: 'https://skill.example.com',
      namespace: 'global',
      slug: `skill-${i}`,
      version: '1.0.0',
      agent: 'claude-code',
      dir: `/home/u/.claude/skills/skill-${i}`,
    }))
    hoisted.state.skills = many
    const { getByText, queryByText } = render(createElement(LocalSkillsPage))

    await waitFor(() => expect(getByText('@global/skill-0')).toBeTruthy())
    // Only the first page is rendered; later items wait on the next page.
    expect(queryByText('@global/skill-9')).toBeNull()

    fireEvent.click(getByText('pagination.next'))

    await waitFor(() => expect(getByText('@global/skill-8')).toBeTruthy())
    expect(getByText('@global/skill-9')).toBeTruthy()
  })
})
