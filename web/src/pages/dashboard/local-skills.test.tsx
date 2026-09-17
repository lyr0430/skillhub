// @vitest-environment jsdom

import { createElement } from 'react'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { LocalSkill } from '@/features/skill/tauri-installer'
import { LocalSkillsPage } from './local-skills'

// Stable `t` handler so it stays referentially equal across renders (real
// react-i18next returns a stable `t`; an inline arrow would re-trigger the
// refresh effect on every render and leave the list stuck loading).
const stableT = (key: string, values?: Record<string, unknown>) =>
  values ? `${key}:${JSON.stringify(values)}` : key

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: stableT }),
}))

const managed = (overrides: Partial<LocalSkill> = {}): LocalSkill => ({
  slug: 'video-frames',
  namespace: 'global',
  version: '1.0.0',
  registry: 'https://skill.example.com',
  origin: 'managed',
  realPath: '/home/u/.claude/skills/video-frames',
  locations: [
    { agent: 'claude-code', path: '/home/u/.claude/skills/video-frames', kind: 'dir' },
  ],
  unreadable: false,
  ...overrides,
})

const hoisted = vi.hoisted(() => {
  const defaultSkills: LocalSkill[] = [
    {
      slug: 'video-frames',
      namespace: 'global',
      version: '1.0.0',
      registry: 'https://skill.example.com',
      origin: 'managed',
      realPath: '/home/u/.claude/skills/video-frames',
      locations: [
        { agent: 'claude-code', path: '/home/u/.claude/skills/video-frames', kind: 'dir' },
      ],
      unreadable: false,
    },
  ]
  const state: {
    isTauri: boolean
    skills: LocalSkill[]
    warnings: string[]
    agents: Array<{ id: string; name: string; dir: string; installed: boolean }>
  } = {
    isTauri: false,
    skills: defaultSkills,
    warnings: [],
    agents: [
      { id: 'claude-code', name: 'Claude Code', dir: '/home/u/.claude/skills', installed: true },
      { id: 'generic', name: '默认全局 Skill 位置', dir: '/home/u/.agents/skills', installed: false },
    ],
  }
  const toastSuccess = vi.fn()
  const toastError = vi.fn()
  const toastWarning = vi.fn()
  const listInstalledSkills = vi.fn().mockImplementation(() =>
    Promise.resolve({ skills: hoisted.state.skills, warnings: hoisted.state.warnings }),
  )
  const installSkill = vi.fn().mockResolvedValue({
    ok: true,
    dir: '/home/u/.claude/skills/video-frames',
    agent: 'claude-code',
    warnings: [],
  })
  const uninstallSkill = vi.fn().mockResolvedValue({
    ok: true,
    agent: 'claude-code',
    dir: '/home/u/.claude/skills/video-frames',
    removedKind: 'dir',
  })
  const attachSkillToAgent = vi.fn().mockResolvedValue({
    ok: true,
    agent: 'generic',
    dir: '/home/u/.agents/skills/video-frames',
    realDir: '/home/u/.claude/skills/video-frames',
    warnings: [],
  })
  const uninstallRepoSkill = vi.fn().mockResolvedValue({
    ok: true,
    removedDir: '/home/u/.skillhub/skills/video-frames',
    removedLinks: ['claude-code'],
    warnings: [],
  })
  const resolveSkillVersion = vi
    .fn()
    .mockResolvedValue({ namespace: 'global', slug: 'video-frames', version: '2.0.0' })
  const openInFileManager = vi.fn().mockResolvedValue(undefined)
  const openExternalUrl = vi.fn().mockResolvedValue(undefined)
  return {
    state,
    defaultSkills,
    toastSuccess,
    toastError,
    toastWarning,
    listInstalledSkills,
    installSkill,
    uninstallSkill,
    uninstallRepoSkill,
    attachSkillToAgent,
    resolveSkillVersion,
    openInFileManager,
    openExternalUrl,
  }
})

// High-level API is mocked so the page logic is testable without the runtime bridge.
vi.mock('@/features/skill/tauri-installer', () => ({
  detectAgents: vi.fn().mockImplementation(() => Promise.resolve(hoisted.state.agents)),
  listInstalledSkills: hoisted.listInstalledSkills,
  uninstallSkill: hoisted.uninstallSkill,
  uninstallRepoSkill: hoisted.uninstallRepoSkill,
  installSkill: hoisted.installSkill,
  attachSkillToAgent: hoisted.attachSkillToAgent,
  openInFileManager: hoisted.openInFileManager,
  openExternalUrl: hoisted.openExternalUrl,
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
    warning: hoisted.toastWarning,
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
    hoisted.state.warnings = []
    window.localStorage.clear()
  })

  const renderDesktop = () => {
    hoisted.state.isTauri = true
    return render(createElement(LocalSkillsPage))
  }

  it('renders a desktop-only notice in the browser', () => {
    hoisted.state.isTauri = false
    const { getByText } = render(createElement(LocalSkillsPage))
    expect(getByText('localSkills.desktopOnly')).toBeTruthy()
  })

  it('lists installed local skills', async () => {
    const { getByText } = renderDesktop()

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    expect(getByText('/home/u/.claude/skills/video-frames')).toBeTruthy()
  })

  // The refresh effect depends on `t`, which real react-i18next returns as a
  // stable identity but can change on a language switch. If it ever became
  // per-render, the page would re-scan the disk forever while still rendering —
  // a symptom no assertion above would catch.
  it('scans the disk once per mount', async () => {
    const { getByText } = renderDesktop()

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    expect(hoisted.listInstalledSkills).toHaveBeenCalledTimes(1)
  })

  // The page used to only show skills carrying skillhub metadata, which hid
  // everything a user had copied in or installed some other way.
  it('shows skills that skillhub did not install, without an update action', async () => {
    hoisted.state.skills = [
      managed({ slug: 'hand-copied', namespace: undefined, origin: 'unmanaged', realPath: '/home/u/.claude/skills/hand-copied', locations: [{ agent: 'claude-code', path: '/home/u/.claude/skills/hand-copied', kind: 'dir' }] }),
    ]
    const { getByText, getByTestId, queryByTestId } = renderDesktop()

    await waitFor(() => expect(getByText('hand-copied')).toBeTruthy())
    expect(getByText('localSkills.originUnmanaged')).toBeTruthy()
    // No source to update from, so the action is not offered at all.
    expect(queryByTestId('local-update-hand-copied')).toBeNull()
    expect(getByTestId('local-uninstall-hand-copied')).toBeTruthy()
  })

  // A real directory reachable from two agents is one skill, not two.
  it('renders one card with both locations when agents share a real directory', async () => {
    hoisted.state.skills = [
      managed({
        locations: [
          {
            agent: 'claude-code',
            path: '/home/u/.claude/skills/shared',
            kind: 'symlink',
            target: '../../.agents/skills/shared',
          },
          { agent: 'generic', path: '/home/u/.agents/skills/shared', kind: 'dir' },
        ],
        realPath: '/home/u/.agents/skills/shared',
      }),
    ]
    const { getAllByText, getByText, getByTestId } = renderDesktop()

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    // Both agent chips on the one card.
    expect(getAllByText('claude-code').length).toBe(1)
    expect(getAllByText('generic').length).toBe(1)
    // The symlink is called out both on the title badge and on its own chip.
    expect(getByTestId('local-symlink-badge-video-frames')).toBeTruthy()
    expect(getAllByText('localSkills.kindSymlink').length).toBe(2)
  })

  // A symlinked skill is updated and removed at its target, so the title has to
  // say so before the user clicks anything.
  it('marks a symlinked skill on its title, and a plain one not at all', async () => {
    hoisted.state.skills = [
      managed({
        slug: 'linked',
        realPath: '/home/u/.agents/skills/linked',
        locations: [
          {
            agent: 'claude-code',
            path: '/home/u/.claude/skills/linked',
            kind: 'symlink',
            target: '../../.agents/skills/linked',
          },
        ],
      }),
      managed({
        slug: 'plain',
        realPath: '/home/u/.claude/skills/plain',
        locations: [
          { agent: 'claude-code', path: '/home/u/.claude/skills/plain', kind: 'dir' },
        ],
      }),
    ]
    const { getByText, getByTestId, queryByTestId } = renderDesktop()

    await waitFor(() => expect(getByText('@global/linked')).toBeTruthy())
    expect(getByTestId('local-symlink-badge-linked')).toBeTruthy()
    expect(queryByTestId('local-symlink-badge-plain')).toBeNull()
  })

  it('opens a skill folder in the system file manager', async () => {
    const { getByTestId } = renderDesktop()

    await waitFor(() => expect(getByTestId('local-open-video-frames')).toBeTruthy())

    fireEvent.click(getByTestId('local-open-video-frames'))

    await waitFor(() => {
      expect(hoisted.openInFileManager).toHaveBeenCalledWith(
        '/home/u/.claude/skills/video-frames',
      )
    })
  })

  it('updates to the latest version when a newer one exists', async () => {
    const { getByTestId, getByText } = renderDesktop()

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    hoisted.resolveSkillVersion.mockResolvedValueOnce({
      namespace: 'global',
      slug: 'video-frames',
      version: '2.0.0',
    })

    fireEvent.click(getByTestId('local-update-video-frames'))

    await waitFor(() => {
      // `dir` names the entry on disk so Rust resolves any symlink itself.
      expect(hoisted.installSkill).toHaveBeenCalledWith(
        {
          namespace: 'global',
          slug: 'video-frames',
          version: '2.0.0',
          agent: 'claude-code',
          dir: '/home/u/.claude/skills/video-frames',
        },
        'https://skill.example.com',
      )
    })
  })

  it('shows a source link only when the skill has one', async () => {
    const { getByTestId, queryByTestId } = renderDesktop()
    await waitFor(() => expect(getByTestId('local-uninstall-video-frames')).toBeTruthy())

    // The default fixture has no homepage, so there is no link to offer.
    expect(queryByTestId('local-homepage-video-frames')).toBeNull()

    cleanup()
    hoisted.state.skills = [
      managed({ homepage: 'https://github.com/team/repo', homepageSource: 'frontmatter' }),
    ]
    const second = renderDesktop()
    await waitFor(() =>
      expect(second.getByTestId('local-homepage-video-frames')).toBeTruthy(),
    )

    fireEvent.click(second.getByTestId('local-homepage-video-frames'))

    await waitFor(() => {
      expect(hoisted.openExternalUrl).toHaveBeenCalledWith('https://github.com/team/repo')
    })
  })

  it('removes only the link and reports the directory that survived', async () => {
    hoisted.state.skills = [
      managed({
        locations: [
          {
            agent: 'claude-code',
            path: '/home/u/.claude/skills/shared',
            kind: 'symlink',
            target: '../../.agents/skills/shared',
          },
        ],
        realPath: '/home/u/.agents/skills/shared',
      }),
    ]
    hoisted.uninstallSkill.mockResolvedValueOnce({
      ok: true,
      agent: 'claude-code',
      dir: '/home/u/.claude/skills/shared',
      removedKind: 'symlink',
      realPathKept: '/home/u/.agents/skills/shared',
    })
    const { getByTestId, getByText } = renderDesktop()

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    fireEvent.click(getByTestId('local-uninstall-video-frames'))

    // The wording must make clear that the real directory is untouched.
    expect(getByText(/localSkills\.effectSymlink/)).toBeTruthy()

    fireEvent.click(getByTestId('confirm-uninstall'))

    await waitFor(() => {
      expect(hoisted.uninstallSkill).toHaveBeenCalledWith(
        '/home/u/.claude/skills/shared',
        'claude-code',
      )
    })
    expect(hoisted.toastSuccess).toHaveBeenCalledWith(
      'localSkills.uninstallSuccess',
      expect.stringContaining('localSkills.uninstallLinkKept'),
    )
  })

  it('makes the user pick a location when a skill lives in several', async () => {
    hoisted.state.skills = [
      managed({
        locations: [
          { agent: 'claude-code', path: '/home/u/.claude/skills/shared', kind: 'symlink' },
          { agent: 'generic', path: '/home/u/.agents/skills/shared', kind: 'dir' },
        ],
        realPath: '/home/u/.agents/skills/shared',
      }),
    ]
    const { getByTestId, getByText } = renderDesktop()

    await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
    fireEvent.click(getByTestId('local-uninstall-video-frames'))

    expect(getByText('localSkills.chooseLocation')).toBeTruthy()

    // Nothing is preselected: for a shared real directory, whichever location
    // happened to sort first could be the one whose removal deletes the skill
    // for every agent. The user has to pick.
    const confirm = getByTestId('confirm-uninstall') as HTMLButtonElement
    expect(confirm.disabled).toBe(true)

    // Pick the second location explicitly; the page must honour the choice
    // rather than silently removing the first one.
    fireEvent.click(getByTestId('local-location-generic'))
    expect(confirm.disabled).toBe(false)
    fireEvent.click(confirm)

    await waitFor(() => {
      expect(hoisted.uninstallSkill).toHaveBeenCalledWith(
        '/home/u/.agents/skills/shared',
        'generic',
      )
    })
  })

  // Regression: a symlinked skill in one agent (e.g. `@global/zero-slop` in
  // `openclaw` pointing into `.skillhub`) must not offer the shared repository
  // directory as a selectable location in the per-agent uninstall dialog. The
  // `.skillhub` real dir is removed only by the repo-category uninstall; a
  // per-agent uninstall just detaches that agent's link.
  it('does not offer the .skillhub repo as a location in the per-agent uninstall dialog', async () => {
    hoisted.state.skills = [
      managed({
        slug: 'zero-slop',
        namespace: undefined,
        realPath: '/home/u/.skillhub/skills/zero-slop',
        repoManaged: true,
        linkedAgents: ['openclaw'],
        locations: [
          { agent: '.skillhub', path: '/home/u/.skillhub/skills/zero-slop', kind: 'dir' },
          { agent: 'openclaw', path: '/home/u/.openclaw/skills/zero-slop', kind: 'symlink' },
        ],
      }),
    ]
    const { getByTestId, queryByTestId, getByText } = renderDesktop()

    await waitFor(() => expect(getByText('zero-slop')).toBeTruthy())
    fireEvent.click(getByTestId('local-uninstall-zero-slop'))

    // The `.skillhub` repo is never offered as a selectable location.
    expect(queryByTestId('local-location-.skillhub')).toBeNull()
    // There is exactly one real agent location (openclaw), so the dialog shows
    // no location chooser and is ready to confirm immediately.
    expect(queryByTestId('local-location-openclaw')).toBeNull()
    const confirm = getByTestId('confirm-uninstall') as HTMLButtonElement
    expect(confirm.disabled).toBe(false)

    fireEvent.click(confirm)
    await waitFor(() => {
      expect(hoisted.uninstallSkill).toHaveBeenCalledWith(
        '/home/u/.openclaw/skills/zero-slop',
        'openclaw',
      )
    })
  })

  it('explains that an unmanaged directory is backed up, not deleted', async () => {
    hoisted.state.skills = [
      managed({
        slug: 'hand-copied',
        namespace: undefined,
        origin: 'unmanaged',
        locations: [
          { agent: 'claude-code', path: '/home/u/.claude/skills/hand-copied', kind: 'dir' },
        ],
        realPath: '/home/u/.claude/skills/hand-copied',
      }),
    ]
    const { getByTestId, getByText } = renderDesktop()

    await waitFor(() => expect(getByText('hand-copied')).toBeTruthy())
    fireEvent.click(getByTestId('local-uninstall-hand-copied'))

    expect(getByText(/localSkills\.effectDir/)).toBeTruthy()
  })

  it('offers to clean up a dangling link', async () => {
    hoisted.state.skills = [
      managed({
        slug: 'stale',
        namespace: undefined,
        origin: 'unmanaged',
        realPath: undefined,
        locations: [
          { agent: 'claude-code', path: '/home/u/.claude/skills/stale', kind: 'broken-symlink' },
        ],
      }),
    ]
    const { getByTestId, getByText } = renderDesktop()

    await waitFor(() => expect(getByText('stale')).toBeTruthy())
    expect(getByText('localSkills.brokenLink')).toBeTruthy()

    fireEvent.click(getByTestId('local-uninstall-stale'))

    expect(getByText(/localSkills\.effectBroken/)).toBeTruthy()
    // No resolvable directory, so there is nothing to open or update.
    expect(getByText('localSkills.noUpdateUnmanagedShort')).toBeTruthy()
  })

  it('surfaces skills roots that could not be read', async () => {
    hoisted.state.warnings = ['无法读取 /home/u/.claude/skills: permission denied']
    const { getByText } = renderDesktop()

    await waitFor(() =>
      expect(getByText('无法读取 /home/u/.claude/skills: permission denied')).toBeTruthy(),
    )
  })

  it('filters the list by search query', async () => {    hoisted.state.skills = [
      managed({ slug: 'alpha' }),
      managed({ slug: 'beta', realPath: '/home/u/.claude/skills/beta', locations: [{ agent: 'claude-code', path: '/home/u/.claude/skills/beta', kind: 'dir' }] }),
    ]
    const { getByPlaceholderText, getByText, queryByText } = renderDesktop()

    await waitFor(() => expect(getByText('@global/alpha')).toBeTruthy())
    expect(getByText('@global/beta')).toBeTruthy()

    fireEvent.change(getByPlaceholderText('localSkills.searchPlaceholder'), {
      target: { value: 'alpha' },
    })

    expect(getByText('@global/alpha')).toBeTruthy()
    expect(queryByText('@global/beta')).toBeNull()
  })

  it('paginates the list beyond the page size', async () => {
    hoisted.state.skills = Array.from({ length: 10 }, (_, i) =>
      managed({
        slug: `skill-${i}`,
        realPath: `/home/u/.claude/skills/skill-${i}`,
        locations: [
          { agent: 'claude-code', path: `/home/u/.claude/skills/skill-${i}`, kind: 'dir' },
        ],
      }),
    )
    const { getByText, queryByText } = renderDesktop()

    await waitFor(() => expect(getByText('@global/skill-0')).toBeTruthy())
    // Only the first page is rendered; later items wait on the next page.
    expect(queryByText('@global/skill-9')).toBeNull()

    fireEvent.click(getByText('pagination.next'))

    await waitFor(() => expect(getByText('@global/skill-8')).toBeTruthy())
    expect(getByText('@global/skill-9')).toBeTruthy()
  })

  describe('attach to another agent', () => {
    // The default fixture lives in claude-code only, and detectAgents reports
    // claude-code + generic, so `generic` is the one remaining candidate.
    it('offers the attach action when another agent could receive the skill', async () => {
      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-attach-video-frames')).toBeTruthy())
    })

    it('hides the attach action when every known agent already has the skill', async () => {
      hoisted.state.skills = [
        managed({
          locations: [
            { agent: 'claude-code', path: '/home/u/.claude/skills/video-frames', kind: 'dir' },
            { agent: 'generic', path: '/home/u/.agents/skills/video-frames', kind: 'dir' },
          ],
        }),
      ]
      const { getByText, queryByTestId } = renderDesktop()

      await waitFor(() => expect(getByText('@global/video-frames')).toBeTruthy())
      expect(queryByTestId('local-attach-video-frames')).toBeNull()
    })

    it('lists only the agents that do not already hold the skill', async () => {
      const { getByTestId, queryByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-attach-video-frames')).toBeTruthy())
      fireEvent.click(getByTestId('local-attach-video-frames'))

      expect(getByTestId('attach-target-generic')).toBeTruthy()
      // claude-code already has it, so it must not be offered.
      expect(queryByTestId('attach-target-claude-code')).toBeNull()
    })

    it('keeps confirm disabled until an agent is picked', async () => {
      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-attach-video-frames')).toBeTruthy())
      fireEvent.click(getByTestId('local-attach-video-frames'))

      expect((getByTestId('confirm-attach') as HTMLButtonElement).disabled).toBe(true)

      fireEvent.click(getByTestId('attach-target-generic'))

      expect((getByTestId('confirm-attach') as HTMLButtonElement).disabled).toBe(false)
    })

    it('attaches to each selected agent and refreshes the list', async () => {
      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-attach-video-frames')).toBeTruthy())
      fireEvent.click(getByTestId('local-attach-video-frames'))
      fireEvent.click(getByTestId('attach-target-generic'))
      fireEvent.click(getByTestId('confirm-attach'))

      await waitFor(() => expect(hoisted.attachSkillToAgent).toHaveBeenCalledTimes(1))
      expect(hoisted.attachSkillToAgent).toHaveBeenCalledWith({
        sourceDir: '/home/u/.claude/skills/video-frames',
        agent: 'generic',
        mode: 'shared',
      })
      await waitFor(() => expect(hoisted.toastSuccess).toHaveBeenCalled())
    })

    it('uses the stored install mode for the attach', async () => {
      window.localStorage.setItem('skillhub-install-mode', 'copy')
      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-attach-video-frames')).toBeTruthy())
      fireEvent.click(getByTestId('local-attach-video-frames'))
      fireEvent.click(getByTestId('attach-target-generic'))
      fireEvent.click(getByTestId('confirm-attach'))

      await waitFor(() => expect(hoisted.attachSkillToAgent).toHaveBeenCalled())
      expect(hoisted.attachSkillToAgent).toHaveBeenCalledWith(
        expect.objectContaining({ mode: 'copy' }),
      )
    })

    it('reports one agent failing without abandoning the others', async () => {
      // Two candidates, so "the others" is a real claim: the first rejects, the
      // second must still be attempted.
      hoisted.state.agents = [
        ...hoisted.state.agents,
        { id: 'codex', name: 'Codex', dir: '/home/u/.codex/skills', installed: true },
      ]
      hoisted.attachSkillToAgent.mockImplementation(({ agent }: { agent: string }) =>
        agent === 'generic'
          ? Promise.reject(new Error('permission denied'))
          : Promise.resolve({
              ok: true,
              agent,
              dir: `/home/u/.codex/skills/video-frames`,
              realDir: '/home/u/.claude/skills/video-frames',
              warnings: [],
            }),
      )

      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-attach-video-frames')).toBeTruthy())
      fireEvent.click(getByTestId('local-attach-video-frames'))
      fireEvent.click(getByTestId('attach-target-generic'))
      fireEvent.click(getByTestId('attach-target-codex'))
      fireEvent.click(getByTestId('confirm-attach'))

      await waitFor(() => expect(hoisted.toastError).toHaveBeenCalled())
      // Both were attempted, not just the first.
      expect(hoisted.attachSkillToAgent).toHaveBeenCalledTimes(2)
      // The dialog closes and the page still refreshes.
      expect(hoisted.listInstalledSkills).toHaveBeenCalledTimes(2)
    })
  })

  describe('.skillhub repository category', () => {
    const repoSkill = (overrides: Partial<LocalSkill> = {}): LocalSkill =>
      managed({
        slug: 'weather',
        namespace: undefined,
        realPath: '/home/u/.skillhub/skills/weather',
        repoManaged: true,
        linkedAgents: ['claude-code', 'codex'],
        locations: [
          { agent: 'claude-code', path: '/home/u/.claude/skills/weather', kind: 'symlink' },
          { agent: 'codex', path: '/home/u/.codex/skills/weather', kind: 'symlink' },
          { agent: '.skillhub', path: '/home/u/.skillhub/skills/weather', kind: 'dir' },
        ],
        ...overrides,
      })

    it('renders the .skillhub category first', async () => {
      const { getByTestId } = renderDesktop()
      await waitFor(() => expect(getByTestId('local-agent-all')).toBeTruthy())

      const repoButton = getByTestId('local-agent-.skillhub')
      const allButton = getByTestId('local-agent-all')
      // The repo category is rendered before "all" in the DOM.
      expect(
        repoButton.compareDocumentPosition(allButton) &
          (window.Node.DOCUMENT_POSITION_FOLLOWING),
      ).toBeTruthy()
    })

    it('filters the list to repo-managed skills when selected', async () => {
      hoisted.state.skills = [
        repoSkill(),
        managed({
          slug: 'local-only',
          namespace: undefined,
          realPath: '/home/u/.claude/skills/local-only',
        }),
      ]
      const { getByTestId, getByText, queryByText } = renderDesktop()

      await waitFor(() => expect(getByText('weather')).toBeTruthy())
      fireEvent.click(getByTestId('local-agent-.skillhub'))

      // Only the repo-managed skill is shown.
      expect(getByText('weather')).toBeTruthy()
      expect(queryByText('local-only')).toBeNull()
    })

    it('shows an empty state when the repo category has no skills', async () => {
      hoisted.state.skills = [
        managed({ slug: 'local-only', namespace: undefined }),
      ]
      const { getByTestId, getByText } = renderDesktop()

      await waitFor(() => expect(getByText('local-only')).toBeTruthy())
      fireEvent.click(getByTestId('local-agent-.skillhub'))

      expect(getByText('localSkills.empty')).toBeTruthy()
    })

    it('shows a ref badge and the linked agents on a repo skill card', async () => {
      hoisted.state.skills = [repoSkill()]
      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-agent-.skillhub')).toBeTruthy())
      fireEvent.click(getByTestId('local-agent-.skillhub'))

      await waitFor(() => expect(getByTestId('local-ref-badge-weather')).toBeTruthy())
      // The linked agents line lists who shares the skill.
      expect(getByTestId('local-linked-agents-weather')).toBeTruthy()
    })

    it('opens a repo-uninstall confirm dialog listing affected agents', async () => {
      hoisted.state.skills = [repoSkill()]
      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-agent-.skillhub')).toBeTruthy())
      fireEvent.click(getByTestId('local-agent-.skillhub'))
      await waitFor(() => expect(getByTestId('local-repo-uninstall-weather')).toBeTruthy())
      fireEvent.click(getByTestId('local-repo-uninstall-weather'))

      // The dialog shows the affected agents.
      expect(getByTestId('repo-affected-claude-code')).toBeTruthy()
      expect(getByTestId('repo-affected-codex')).toBeTruthy()
    })

    it('does not call uninstallRepoSkill when the confirm dialog is cancelled', async () => {
      hoisted.state.skills = [repoSkill()]
      const { getByTestId, getByText } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-agent-.skillhub')).toBeTruthy())
      fireEvent.click(getByTestId('local-agent-.skillhub'))
      await waitFor(() => expect(getByTestId('local-repo-uninstall-weather')).toBeTruthy())
      fireEvent.click(getByTestId('local-repo-uninstall-weather'))
      fireEvent.click(getByText('localSkills.cancel'))

      expect(hoisted.uninstallRepoSkill).not.toHaveBeenCalled()
    })

    it('calls uninstallRepoSkill with the slug and refreshes on confirm', async () => {
      hoisted.state.skills = [repoSkill()]
      const { getByTestId } = renderDesktop()

      await waitFor(() => expect(getByTestId('local-agent-.skillhub')).toBeTruthy())
      fireEvent.click(getByTestId('local-agent-.skillhub'))
      await waitFor(() => expect(getByTestId('local-repo-uninstall-weather')).toBeTruthy())
      fireEvent.click(getByTestId('local-repo-uninstall-weather'))
      fireEvent.click(getByTestId('confirm-repo-uninstall'))

      await waitFor(() => {
        expect(hoisted.uninstallRepoSkill).toHaveBeenCalledWith('weather')
      })
      await waitFor(() => expect(hoisted.toastSuccess).toHaveBeenCalled())
      // The list re-scans silently after removal.
      expect(hoisted.listInstalledSkills).toHaveBeenCalledTimes(2)
    })

    it('renders the .skillhub category button even though it is not an install target', async () => {
      // The category button renders on the page with its own icon.
      const { getByTestId } = renderDesktop()
      await waitFor(() => expect(getByTestId('local-agent-.skillhub')).toBeTruthy())
      expect(getByTestId('local-agent-.skillhub').textContent).toContain(
        'localSkills.repoCategory',
      )

      // The install dialog's agent list comes from detectAgents(), which never
      // includes .skillhub — it is a display-only classifier, never installable.
      const targets = hoisted.state.agents
      expect(targets.every((agent) => agent.id !== '.skillhub')).toBe(true)
    })

    // Regression: `.skillhub` sorts before letters in `locations`, so a naive
    // `locations[0]` would pick it as the update target and send a repo path that
    // `ensure_under_agent_root` rejects. Update must prefer a real agent location.
    it('updates a shared skill through a real agent, not the .skillhub classifier', async () => {
      hoisted.state.skills = [
        managed({
          slug: 'weather',
          namespace: 'global',
          version: '1.0.0',
          realPath: '/home/u/.skillhub/skills/weather',
          repoManaged: true,
          linkedAgents: ['claude-code', 'codex'],
          // `.skillhub` location is first, mirroring the aggregated sort order.
          locations: [
            { agent: '.skillhub', path: '/home/u/.skillhub/skills/weather', kind: 'dir' },
            { agent: 'claude-code', path: '/home/u/.claude/skills/weather', kind: 'symlink' },
          ],
        }),
      ]
      const { getByTestId } = renderDesktop()
      await waitFor(() => expect(getByTestId('local-update-weather')).toBeTruthy())

      fireEvent.click(getByTestId('local-update-weather'))

      await waitFor(() => {
        expect(hoisted.installSkill).toHaveBeenCalledWith(
          expect.objectContaining({
            agent: 'claude-code',
            dir: '/home/u/.claude/skills/weather',
          }),
          expect.any(String),
        )
      })
      // Never picks `.skillhub` as the update agent.
      const call = hoisted.installSkill.mock.calls[0][0]
      expect(call.agent).not.toBe('.skillhub')
    })
  })
})
