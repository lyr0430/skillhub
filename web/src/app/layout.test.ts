// @vitest-environment jsdom

import { createElement } from 'react'
import { render } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

// Layout is a component-only file with no exported pure functions or constants.
// We verify that the named export exists for the router to consume, and that it
// still renders the global footer.

vi.mock('@tanstack/react-router', async () => {
  const { createElement } = await import('react')
  return {
    Outlet: () => null,
    // Render the destination so footer assertions can see router links; the
    // real Link only carries `to`/`search` as props, not as an href.
    Link: ({ children, to }: { children?: unknown; to?: string }) =>
      createElement('a', { href: to }, children as never),
    useRouterState: () => ({ pathname: '/', resolvedPathname: '/' }),
  }
})

vi.mock('react-i18next', async () => {
  const actual = await vi.importActual<typeof import('react-i18next')>('react-i18next')
  return {
    ...actual,
    useTranslation: () => ({
      t: (key: string) => key,
      i18n: { language: 'en' },
    }),
  }
})

vi.mock('@/features/auth/use-auth', () => ({
  useAuth: () => ({
    user: null,
    isLoading: false,
  }),
}))

vi.mock('@/shared/components/language-switcher', () => ({
  LanguageSwitcher: () => null,
}))

vi.mock('@/shared/components/user-menu', () => ({
  UserMenu: () => null,
}))

vi.mock('@/features/notification/notification-bell', () => ({
  NotificationBell: () => null,
}))

// The sidebar pulls in the whole console navigation tree; irrelevant here.
vi.mock('@/pages/dashboard', () => ({
  DashboardSidebar: () => null,
  SIDEBAR_GROUPS: [],
}))

vi.mock('./layout-header-style', () => ({
  getAppHeaderClassName: () => 'header-class',
}))

vi.mock('./layout-main-content', () => ({
  resolveAppMainContentPathname: (p: string) => p,
  getAppMainContentLayout: () => ({
    mainClassName: 'main-class',
    contentClassName: 'content-class',
  }),
}))

import { Layout } from './layout'

describe('Layout', () => {
  it('exports a named Layout component function', () => {
    expect(typeof Layout).toBe('function')
    expect(Layout.name).toBe('Layout')
  })

  // The footer was once commented out wholesale while adding an unrelated
  // feature, which silently removed the Privacy / Terms / License / GitHub
  // entry points from every public page. Nothing caught it, so the regression
  // is pinned here: this renders the shell and asserts the footer is really in
  // the output, not merely referenced.
  it('renders the global footer with its legal and project links', () => {
    const { container } = render(createElement(Layout))

    const footer = container.querySelector('footer')
    expect(footer, 'the global footer must be rendered').not.toBeNull()

    const html = footer?.innerHTML ?? ''
    for (const [label, fragment] of [
      ['privacy', '/privacy'],
      ['terms', '/terms'],
      ['license', 'blob/main/LICENSE'],
      ['github', 'github.com/iflytek/skillhub'],
    ] as const) {
      expect(html, `the footer must keep its ${label} entry point`).toContain(fragment)
    }
  })

  // Same failure mode, one level down: the footer's own dependencies were
  // dropped by a later upstream merge, so un-commenting the markup alone would
  // not compile. Rendering it proves the class constant and brand mark resolve.
  it('renders the footer brand mark and its link styling', () => {
    const { container } = render(createElement(Layout))

    const footer = container.querySelector('footer')
    expect(footer?.querySelector('img[src*="favicon"]')).not.toBeNull()
    expect(footer?.querySelector('a')).not.toBeNull()
  })
})
