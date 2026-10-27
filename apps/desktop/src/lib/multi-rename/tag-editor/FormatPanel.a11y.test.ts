/** Tier 3 a11y tests for `FormatPanel.svelte`: default, filled, with a mask error showing, and locked. */
import { afterEach, describe, it } from 'vitest'
import FormatPanel from './FormatPanel.svelte'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { asTags, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountPanel(disabled = false) {
  const tags = fakeTags()
  const mounted = await mountInto(FormatPanel, { disabled, state: asTags(tags), runScoped: fakeRunScoped() })
  cleanup = mounted.cleanup
  return { root: mounted.root, tags }
}

describe('FormatPanel a11y', () => {
  it('default has no violations', async () => {
    await expectNoA11yViolations((await mountPanel()).root)
  })

  it('filled has no violations', async () => {
    const { root } = await mountPanel()
    await typeInto(byId<HTMLInputElement>(root, 'tag-format-mask'), '[N3-]')
    await expectNoA11yViolations(root)
  })

  it('with a mask error has no violations', async () => {
    const { root, tags } = await mountPanel()
    tags.textTool.mockImplementation(() => {
      tags.error = { kind: 'badMask', error: { type: 'unclosed', at: 0 } }
      return Promise.resolve()
    })
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Format')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await expectNoA11yViolations(root)
  })

  it('locked has no violations', async () => {
    await expectNoA11yViolations((await mountPanel(true)).root)
  })
})
