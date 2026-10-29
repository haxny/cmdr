/** Find and replace in tags: the exact tool it sends, its fields, options, and its worded errors. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import ReplacePanel from './ReplacePanel.svelte'
import type { TagField } from '#lib/ipc/bindings.js'
import { TAG_FIELDS } from './field-labels'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { SCOPE, asTags, boxNamed, buttonNamed, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'

const ipc = vi.hoisted(() => ({
  getTagReplacePresets: vi.fn(),
  saveTagReplacePreset: vi.fn(),
  deleteTagReplacePreset: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', async (original) => ({ ...(await original<object>()), ...ipc }))

const CZECH = {
  id: 'czech',
  name: 'Czech part words → numbers',
  steps: [{ fields: ['title'], search: 'x', replace: ' 01', caseSensitive: false, firstOnly: false, regex: true }],
}

let cleanup: (() => void) | undefined
beforeEach(() => {
  ipc.getTagReplacePresets.mockResolvedValue([CZECH])
})
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountPanel(over: { disabled?: boolean; scope?: string[] | null; focusedField?: TagField | null } = {}) {
  const tags = fakeTags()
  const runScoped = fakeRunScoped(over.scope === undefined ? SCOPE : over.scope)
  const mounted = await mountInto(ReplacePanel, {
    disabled: over.disabled ?? false,
    state: asTags(tags),
    runScoped,
    focusedField: over.focusedField ?? null,
  })
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

  it('preselects the focused field when it opens, Title otherwise', async () => {
    const { root, tags } = await mountPanel({ focusedField: 'album' })
    await fill(root, 'a', 'b')
    await click(buttonNamed(root, 'Replace'))
    expect(tags.textTool).toHaveBeenCalledWith(expect.objectContaining({ fields: ['album'] }), SCOPE)
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

  it('chip tooltips advertise no ⌘⌥ key, which reaches the Rename tab’s options, not these', async () => {
    const { root } = await mountPanel()
    expect(root.querySelector('.search-option-tip')).not.toBeNull()
    expect(root.querySelector('.search-option-tip kbd')).toBeNull()
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

  describe('saved lists', () => {
    const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

    it('shows the saved lists and runs one as a single steps tool on the scope', async () => {
      const { root, tags } = await mountPanel()
      await settle()
      await click(root.querySelector<HTMLButtonElement>('button[aria-label="Run Czech part words → numbers"]')!)
      expect(tags.textTool).toHaveBeenCalledTimes(1)
      expect(tags.textTool).toHaveBeenCalledWith({ kind: 'steps', steps: CZECH.steps }, SCOPE)
    })

    it('runs on every track when none is checked', async () => {
      const { root, tags } = await mountPanel({ scope: null })
      await settle()
      await click(root.querySelector<HTMLButtonElement>('button[aria-label^="Run "]')!)
      expect(tags.textTool).toHaveBeenCalledWith(expect.anything(), null)
    })

    it('collects steps and saves them under a name', async () => {
      ipc.saveTagReplacePreset.mockImplementation((preset: unknown) => Promise.resolve([preset]))
      const { root } = await mountPanel()
      await fill(root, ', part one', ' 01')
      await click(buttonNamed(root, 'Add as step'))
      expect(byId<HTMLInputElement>(root, 'tag-replace-search').value).toBe('')
      await fill(root, ', part two', ' 02')
      await typeInto(byId<HTMLInputElement>(root, 'tag-replace-list-name'), 'Parts')
      await click(buttonNamed(root, 'Save list'))
      const saved = ipc.saveTagReplacePreset.mock.calls[0]?.[0]
      expect(saved).toMatchObject({ name: 'Parts' })
      expect(saved.steps.map((s: { search: string }) => s.search)).toEqual([', part one', ', part two'])
      expect(saved.steps[0]).toMatchObject({ fields: ['title'], replace: ' 01', regex: false })
    })

    it('saves all-fields steps as null fields so new fields are covered', async () => {
      ipc.saveTagReplacePreset.mockResolvedValue([])
      const { root } = await mountPanel()
      await click(boxNamed(root, 'All fields'))
      await fill(root, 'a', 'b')
      await typeInto(byId<HTMLInputElement>(root, 'tag-replace-list-name'), 'All')
      await click(buttonNamed(root, 'Save list'))
      expect(ipc.saveTagReplacePreset.mock.calls[0]?.[0].steps[0].fields).toBeNull()
    })

    it('does not save without a name or without a step', async () => {
      const { root } = await mountPanel()
      expect(buttonNamed(root, 'Save list').disabled).toBe(true)
      await typeInto(byId<HTMLInputElement>(root, 'tag-replace-list-name'), 'Parts')
      expect(buttonNamed(root, 'Save list').disabled).toBe(true)
      await fill(root, 'a', 'b')
      expect(buttonNamed(root, 'Save list').disabled).toBe(false)
    })

    it('deletes a list', async () => {
      ipc.deleteTagReplacePreset.mockResolvedValue([])
      const { root } = await mountPanel()
      await settle()
      await click(root.querySelector<HTMLButtonElement>('button[aria-label="Delete Czech part words → numbers"]')!)
      expect(ipc.deleteTagReplacePreset).toHaveBeenCalledWith('czech')
      expect(root.textContent).toContain('No saved lists yet.')
    })

    it('words a refused save', async () => {
      const { ReplacePresetRefused } = await import('#lib/tauri-commands/index.js')
      ipc.saveTagReplacePreset.mockRejectedValue(new ReplacePresetRefused({ type: 'listFull', max: 200 }))
      const { root } = await mountPanel()
      await fill(root, 'a', 'b')
      await typeInto(byId<HTMLInputElement>(root, 'tag-replace-list-name'), 'Parts')
      await click(buttonNamed(root, 'Save list'))
      expect(root.textContent).toContain('too many saved lists')
    })

    it('says so when the lists will not load, and still replaces', async () => {
      ipc.getTagReplacePresets.mockRejectedValue(new Error('no'))
      const { root, tags } = await mountPanel()
      await settle()
      expect(root.textContent).toContain('Couldn’t load the saved lists.')
      await fill(root, 'a', 'b')
      await click(buttonNamed(root, 'Replace'))
      expect(tags.textTool).toHaveBeenCalled()
    })
  })
})
