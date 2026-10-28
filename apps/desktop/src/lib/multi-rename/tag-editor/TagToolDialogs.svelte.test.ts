/**
 * The tag tools' dialogs: each sends its exact tool, Renumber and Remove extras follow the tag canon, Case and Clear
 * start on the field the user was on, and the one-press tools close their dialog once run.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { TagField } from '#lib/ipc/bindings.js'
import TagToolDialogs from './TagToolDialogs.svelte'
import type { TagToolId } from './tag-tools'
import { DIACRITICS_FIELDS } from './tag-tools'
import { mountInto } from './tag-cell.test-helper'
import { SCOPE, asTags, boxNamed, buttonNamed, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'

vi.mock('#lib/app-status-store.js', () => ({
  TAG_NAME_PATTERNS_MAX: 10,
  loadTagNamePatterns: vi.fn(() => Promise.resolve([])),
  saveTagNamePatterns: vi.fn(() => Promise.resolve()),
  loadTagNumbering: vi.fn(() => Promise.resolve(null)),
  saveTagNumbering: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', async (original) => ({
  ...(await original<object>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountTool(
  open: TagToolId,
  over: { trackCount?: number; extras?: string[]; field?: TagField | null; selectedCount?: number } = {},
) {
  const onTool = vi.fn()
  const onClear = vi.fn()
  const onClose = vi.fn()
  const runScoped = fakeRunScoped()
  const fake = fakeTags()
  const mounted = await mountInto(TagToolDialogs, {
    open,
    field: over.field ?? null,
    trackCount: over.trackCount ?? 3,
    selectedCount: over.selectedCount ?? 0,
    extras: over.extras ?? [],
    state: asTags(fake),
    runScoped,
    scopeKey: 'k',
    onTool,
    onClear,
    onClose,
  })
  cleanup = mounted.cleanup
  return { onTool, onClear, onClose, runScoped, fake }
}

function button(text: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === text)
}

async function click(text: string): Promise<void> {
  const target = button(text)
  if (!target) throw new Error(`no button ${text}`)
  target.click()
  await tick()
}

describe('TagToolDialogs', () => {
  it.each([
    ['replace', 'Find and replace'],
    ['format', 'Format a field'],
    ['fromName', 'Tags from file names'],
    ['number', 'Auto-numbering wizard'],
    ['case', 'Change case'],
    ['cleanUp', 'Clean up'],
    ['extras', 'Remove extra tags'],
    ['clearField', 'Clear a field'],
  ] as const)('%s opens as a dialog titled %s', async (open, title) => {
    await mountTool(open)
    const dialog = document.querySelector('[role="dialog"]')
    expect(dialog).not.toBeNull()
    expect(dialog?.textContent).toContain(title)
  })

  it.each([
    ['Strip track prefix', { kind: 'stripTrackPrefix' }],
    ['Renumber', { kind: 'renumber', start: 1 }],
    ['Album artist = Artist', { kind: 'albumArtistFromArtist' }],
    ['Narrator from text', { kind: 'narratorFromText' }],
    ['Clear title = album', { kind: 'clearRedundantTitles' }],
  ])('Clean up’s %s sends %j and closes', async (text, tool) => {
    const { onTool, onClose } = await mountTool('cleanUp')
    await click(text)
    expect(onTool).toHaveBeenCalledWith(tool)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('Clean up’s Text group defaults to Remove diacritics on all fields but Comment and Description, so one Run is the old button', async () => {
    const { fake, onClose, onTool } = await mountTool('cleanUp')
    expect(boxNamed(document.body, 'Remove diacritics').checked).toBe(true)
    expect(boxNamed(document.body, 'Greek to Latin').checked).toBe(false)
    expect(boxNamed(document.body, 'Normalize Unicode').checked).toBe(false)
    await click('Run')
    expect(fake.textTool).toHaveBeenCalledWith(
      {
        kind: 'text',
        fields: [...DIACRITICS_FIELDS],
        greekToLatin: false,
        removeDiacritics: true,
        normalizeUnicode: false,
      },
      SCOPE,
    )
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(onTool).not.toHaveBeenCalled()
  })

  it('Clean up’s Text group sends all three options in one run on the selected tracks', async () => {
    const { fake } = await mountTool('cleanUp')
    boxNamed(document.body, 'Greek to Latin').click()
    boxNamed(document.body, 'Normalize Unicode').click()
    await tick()
    await click('Run')
    expect(fake.textTool).toHaveBeenCalledWith(
      {
        kind: 'text',
        fields: [...DIACRITICS_FIELDS],
        greekToLatin: true,
        removeDiacritics: true,
        normalizeUnicode: true,
      },
      SCOPE,
    )
  })

  it('Clean up’s Text group runs on the field the dialog opened on when it is chosen, even Comment', async () => {
    const { fake } = await mountTool('cleanUp', { field: 'comment' })
    const trigger = document.querySelector<HTMLElement>('.select-trigger')
    expect(trigger).not.toBeNull()
    trigger?.click()
    await tick()
    await tick()
    const item = [
      ...document.querySelectorAll<HTMLElement>('[data-part="content"][data-state="open"] [data-part="item"]'),
    ].find((el) => el.getAttribute('data-value') === 'comment')
    expect(item).toBeDefined()
    item?.click()
    await tick()
    await tick()
    await click('Run')
    expect(fake.textTool).toHaveBeenCalledWith(
      { kind: 'text', fields: ['comment'], greekToLatin: false, removeDiacritics: true, normalizeUnicode: false },
      SCOPE,
    )
  })

  it('a failed run keeps the dialog open and says so', async () => {
    const { fake, onClose } = await mountTool('cleanUp')
    fake.textTool.mockImplementation(() => {
      fake.error = { kind: 'unknownTrack' }
      return Promise.resolve()
    })
    await click('Run')
    expect(onClose).not.toHaveBeenCalled()
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('Couldn’t change the tags')
  })

  it('Run is disabled with nothing ticked', async () => {
    const { fake } = await mountTool('cleanUp')
    boxNamed(document.body, 'Remove diacritics').click()
    await tick()
    expect(buttonNamed(document.body, 'Run').disabled).toBe(true)
    expect(fake.textTool).not.toHaveBeenCalled()
  })

  it('renumber_hidden_for_single_file', async () => {
    await mountTool('cleanUp', { trackCount: 1 })
    expect(button('Renumber')).toBeUndefined()
  })

  it('Case with no field changes the five name fields, and says so', async () => {
    const { onTool } = await mountTool('case')
    expect(document.body.textContent).toContain('Changes: Title, Album, Artist, Album artist, Performer')
    await click('UPPER CASE')
    expect(onTool).toHaveBeenCalledWith({
      kind: 'case',
      case: 'upper',
      fields: ['title', 'album', 'artist', 'albumArtist', 'performer'],
    })
  })

  it('Case with a field changes that field only', async () => {
    const { onTool } = await mountTool('case', { field: 'album' })
    expect(document.body.textContent).toContain('Changes: Album')
    await click('Sentence case')
    expect(onTool).toHaveBeenCalledWith({ kind: 'case', case: 'sentence', fields: ['album'] })
  })

  it('remove_extras_requires_explicit_choice', async () => {
    const { onTool } = await mountTool('extras', { extras: ['COMM:eng', 'TXXX:MOOD'] })
    const boxes = [...document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')]
    expect(boxes).toHaveLength(2)
    expect(boxes.every((box) => !box.checked)).toBe(true)
    expect(button('Remove 0 extra tags')?.disabled).toBe(true)
    boxes[1].click()
    await tick()
    await click('Remove 1 extra tag')
    expect(onTool).toHaveBeenCalledTimes(1)
    expect(onTool).toHaveBeenCalledWith({ kind: 'removeExtras', keys: ['TXXX:MOOD'] })
  })

  it('Clear starts on the field the tool opened on, names the scope, and clears that field', async () => {
    const { onClear, onClose } = await mountTool('clearField', { field: 'album', selectedCount: 2 })
    expect(document.body.textContent).toContain('Empties Album on the 2 selected tracks.')
    await click('Clear')
    expect(onClear).toHaveBeenCalledWith('album')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('Clear with nothing selected says it empties the field on every file; with no field it starts on Title', async () => {
    await mountTool('clearField')
    expect(document.body.textContent).toContain('Empties Title on every file.')
  })

  it('the text tools stay open to run again and close with Close', async () => {
    const { onClose } = await mountTool('replace')
    await click('Close')
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
