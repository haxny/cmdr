/** Find and replace in tags: the exact tool it sends, its fields, options, and its worded errors. */
import { afterEach, describe, expect, it } from 'vitest'
import { tick } from 'svelte'
import ReplacePanel from './ReplacePanel.svelte'
import { TAG_FIELDS } from './field-labels'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { SCOPE, asTags, boxNamed, buttonNamed, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountPanel(over: { disabled?: boolean; scope?: string[] | null } = {}) {
  const tags = fakeTags()
  const runScoped = fakeRunScoped(over.scope === undefined ? SCOPE : over.scope)
  const mounted = await mountInto(ReplacePanel, { disabled: over.disabled ?? false, state: asTags(tags), runScoped })
  cleanup = mounted.cleanup
  return { root: mounted.root, tags, runScoped }
}

async function fill(root: HTMLElement, search: string, replace: string): Promise<void> {
  await typeInto(byId<HTMLInputElement>(root, 'tag-replace-search'), search)
  await typeInto(byId<HTMLInputElement>(root, 'tag-replace-with'), replace)
}

async function click(el: HTMLElement): Promise<void> {
  el.click()
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('ReplacePanel', () => {
  it('sends the replace tool on Title only, with the checked scope', async () => {
    const { root, tags } = await mountPanel()
    await fill(root, ', cast ', ' 0')
    await click(buttonNamed(root, 'Replace'))
    expect(tags.textTool).toHaveBeenCalledWith(
      {
        kind: 'replace',
        fields: ['title'],
        search: ', cast ',
        replace: ' 0',
        caseSensitive: false,
        firstOnly: false,
        regex: false,
      },
      SCOPE,
    )
  })

  it('goes over every track when none is checked', async () => {
    const { root, tags } = await mountPanel({ scope: null })
    await fill(root, 'a', 'b')
    await click(buttonNamed(root, 'Replace'))
    expect(tags.textTool).toHaveBeenCalledWith(expect.anything(), null)
  })

  it('takes the chosen fields in display order', async () => {
    const { root, tags } = await mountPanel()
    await click(boxNamed(root, 'Album'))
    await click(boxNamed(root, 'Artist'))
    await fill(root, 'a', 'b')
    await click(buttonNamed(root, 'Replace'))
    expect(tags.textTool.mock.calls[0]?.[0]).toMatchObject({ fields: ['title', 'artist', 'album'] })
  })

  it('All fields sends every field', async () => {
    const { root, tags } = await mountPanel()
    await click(boxNamed(root, 'All fields'))
    await fill(root, 'a', 'b')
    await click(buttonNamed(root, 'Replace'))
    expect(tags.textTool.mock.calls[0]?.[0]).toMatchObject({ fields: [...TAG_FIELDS] })
  })

  it('does not run with no field chosen', async () => {
    const { root } = await mountPanel()
    await click(boxNamed(root, 'Title'))
    await fill(root, 'a', 'b')
    expect(buttonNamed(root, 'Replace').disabled).toBe(true)
  })

  it('sends the option chips as toggled', async () => {
    const { root, tags } = await mountPanel()
    for (const name of ['Match case', 'First match only', 'Regular expression']) {
      root.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)?.click()
      await tick()
    }
    await fill(root, '(\\d+)', '$1')
    await click(buttonNamed(root, 'Replace'))
    expect(tags.textTool.mock.calls[0]?.[0]).toMatchObject({ caseSensitive: true, firstOnly: true, regex: true })
  })

  it('offers just the three options a tag can use', async () => {
    const { root } = await mountPanel()
    const chips = [...root.querySelectorAll('button[aria-pressed]')].map((b) => b.getAttribute('aria-label'))
    expect(chips).toEqual(['Match case', 'First match only', 'Regular expression'])
  })

  it('an empty search disables Replace', async () => {
    const { root } = await mountPanel()
    expect(buttonNamed(root, 'Replace').disabled).toBe(true)
    await fill(root, 'x', '')
    expect(buttonNamed(root, 'Replace').disabled).toBe(false)
  })

  it('words a bad regular expression like Multi-rename does', async () => {
    const { root, tags } = await mountPanel()
    tags.textTool.mockImplementation(() => {
      tags.error = { kind: 'badRegex' }
      return Promise.resolve()
    })
    await fill(root, '(', 'x')
    await click(buttonNamed(root, 'Replace'))
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBe('The regular expression isn’t valid')
  })

  it('shows nothing when the checked tracks are gone and the call never ran', async () => {
    const { root, tags, runScoped } = await mountPanel()
    runScoped.mockImplementation(() => Promise.resolve(undefined))
    await fill(root, 'a', 'b')
    await click(buttonNamed(root, 'Replace'))
    expect(tags.textTool).not.toHaveBeenCalled()
    expect(root.querySelector('[role="alert"]')).toBeNull()
  })

  it('a read-only session locks it', async () => {
    const { root } = await mountPanel({ disabled: true })
    await fill(root, 'a', 'b')
    expect(buttonNamed(root, 'Replace').disabled).toBe(true)
  })
})
