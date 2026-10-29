/** Tier 3 a11y tests for `ReplacePanel.svelte`: empty, filled, with an error showing, and locked. */
import { afterEach, describe, it, vi } from 'vitest'
import ReplacePanel from './ReplacePanel.svelte'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { asTags, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/tauri-commands/index.js', async (original) => ({
  ...(await original<object>()),
  getTagReplacePresets: vi.fn(() =>
    Promise.resolve([
      {
        id: 'czech',
        name: 'Czech part words → numbers',
        steps: [
          { fields: ['title'], search: 'x', replace: ' 01', caseSensitive: false, firstOnly: false, regex: true },
        ],
      },
    ]),
  ),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountPanel(disabled = false) {
  const tags = fakeTags()
  const mounted = await mountInto(ReplacePanel, { disabled, state: asTags(tags), runScoped: fakeRunScoped() })
  cleanup = mounted.cleanup
  return { root: mounted.root, tags }
}

describe('ReplacePanel a11y', () => {
  it('empty has no violations', async () => {
    await expectNoA11yViolations((await mountPanel()).root)
  })

  it('filled has no violations', async () => {
    const { root } = await mountPanel()
    await typeInto(byId<HTMLInputElement>(root, 'tag-replace-search'), ', cast ')
    await typeInto(byId<HTMLInputElement>(root, 'tag-replace-with'), ' 0')
    await expectNoA11yViolations(root)
  })

  it('with a bad regular expression has no violations', async () => {
    const { root, tags } = await mountPanel()
    tags.textTool.mockImplementation(() => {
      tags.error = { kind: 'badRegex' }
      return Promise.resolve()
    })
    await typeInto(byId<HTMLInputElement>(root, 'tag-replace-search'), '(')
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Replace')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await expectNoA11yViolations(root)
  })

  it('locked has no violations', async () => {
    await expectNoA11yViolations((await mountPanel(true)).root)
  })

  it('with saved lists and a step in the making has no violations', async () => {
    const { root } = await mountPanel()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await typeInto(byId<HTMLInputElement>(root, 'tag-replace-search'), ', part one')
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Add as step')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await expectNoA11yViolations(root)
  })
})
