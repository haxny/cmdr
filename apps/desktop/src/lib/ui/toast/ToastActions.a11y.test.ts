/**
 * Tier 3 a11y tests for `ToastActions.svelte`.
 *
 * The toast action row: a plain layout `div` with no role or ARIA of its own, so each button keeps
 * its own name and tab stop. axe confirms the row adds nothing that clashes with them.
 */
import { describe, it } from 'vitest'
import { createRawSnippet, mount, tick } from 'svelte'
import ToastActions from './ToastActions.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

describe('ToastActions a11y', () => {
  it('a row of two buttons has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const children = createRawSnippet(() => ({
      render: () => '<span><button type="button">Later</button><button type="button">Restart now</button></span>',
    }))
    mount(ToastActions, { target, props: { children } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
