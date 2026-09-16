// @vitest-environment jsdom

import { createElement, type ReactNode } from 'react'
import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { PORTAL_ROOT_ID } from '@/shared/lib/portal-container'
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
} from './alert-dialog'

afterEach(() => {
  cleanup()
  document.getElementById(PORTAL_ROOT_ID)?.remove()
})

/** Open a dialog into the shared portal host, like the app does. */
function renderDialog(children?: ReactNode): HTMLElement {
  const host = document.createElement('div')
  host.id = PORTAL_ROOT_ID
  document.body.appendChild(host)

  render(
    createElement(
      AlertDialog,
      { open: true },
      createElement(
        AlertDialogContent,
        null,
        createElement(AlertDialogTitle, null, 'Uninstall this skill?'),
        createElement(AlertDialogDescription, null, 'The directory is removed.'),
        children,
      ),
    ),
  )

  return host
}

describe('AlertDialogContent', () => {
  // It used to inject its own `sr-only` Cancel labelled with a hardcoded 取消.
  // Radix focuses that Cancel on open, so keyboard focus landed on a button the
  // user cannot see; en/ru users heard a Chinese label; and screen readers found
  // a cancel entry the caller never wrote. Each caller builds its own footer.
  it('renders only the buttons its caller supplies', () => {
    const host = renderDialog(createElement('button', null, 'keep'))

    const labels = Array.from(host.querySelectorAll('button')).map((button) => button.textContent)
    expect(labels).toEqual(['keep'])
  })

  it('adds no invisible button and no hardcoded cancel label', () => {
    const host = renderDialog()

    expect(host.querySelector('button')).toBeNull()
    expect(host.textContent).not.toContain('取消')
  })
})
