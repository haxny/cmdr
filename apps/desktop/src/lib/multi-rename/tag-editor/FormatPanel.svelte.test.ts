/** Format a field with a Multi-rename mask: the exact tool, the multi-value hint, the worded mask errors. */
import { afterEach, describe, expect, it } from 'vitest'
import { tick } from 'svelte'
import FormatPanel from './FormatPanel.svelte'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { SCOPE, asTags, buttonNamed, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'

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

async function click(el: HTMLElement): Promise<void> {
  el.click()
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('FormatPanel', () => {
  it('starts with the mask [N] on Title', async () => {
    const { root, tags } = await mountPanel()
    expect(byId<HTMLInputElement>(root, 'tag-format-mask').value).toBe('[N]')
    await click(buttonNamed(root, 'Format'))
    expect(tags.textTool).toHaveBeenCalledWith({ kind: 'format', field: 'title', mask: '[N]' }, SCOPE)
  })

  it('sends the typed mask', async () => {
    const { root, tags } = await mountPanel()
    await typeInto(byId<HTMLInputElement>(root, 'tag-format-mask'), '[N3-]')
    await click(buttonNamed(root, 'Format'))
    expect(tags.textTool).toHaveBeenCalledWith({ kind: 'format', field: 'title', mask: '[N3-]' }, SCOPE)
  })

  it('picks the field from the list', async () => {
    const { root, tags } = await mountPanel()
    await click(root.querySelector<HTMLElement>('.select-trigger') as HTMLElement)
    const item = [...document.querySelectorAll<HTMLElement>('.select-item')].find(
      (i) => i.textContent.trim() === 'Album',
    )
    await click(item as HTMLElement)
    await click(buttonNamed(root, 'Format'))
    expect(tags.textTool.mock.calls[0]?.[0]).toMatchObject({ field: 'album' })
  })

  it('says Format keeps only the first value of a multi-value field', async () => {
    const { root } = await mountPanel()
    expect(root.textContent).toContain('Format keeps only the first value of a field that holds several')
  })

  it.each([
    [{ type: 'unclosed', at: 2 } as const, 'A placeholder is missing its closing “]”'],
    [{ type: 'unknown', placeholder: 'Q' } as const, 'Cmdr doesn’t know the placeholder [Q]'],
  ])('words the mask error %j', async (error, text) => {
    const { root, tags } = await mountPanel()
    tags.textTool.mockImplementation(() => {
      tags.error = { kind: 'badMask', error }
      return Promise.resolve()
    })
    await click(buttonNamed(root, 'Format'))
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBe(text)
  })

  it('an empty mask disables Format, and a read-only session locks it', async () => {
    const { root } = await mountPanel()
    await typeInto(byId<HTMLInputElement>(root, 'tag-format-mask'), '')
    expect(buttonNamed(root, 'Format').disabled).toBe(true)
    cleanup?.()
    const locked = await mountPanel(true)
    expect(buttonNamed(locked.root, 'Format').disabled).toBe(true)
  })
})
