/** From file name: pattern or regex mode, a preview table before taking, the recent list, worded pattern errors. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { NamePreviewRow } from '#lib/tauri-commands/index.js'
import NamePatternPanel from './NamePatternPanel.svelte'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { SCOPE, asTags, buttonNamed, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'

const store = vi.hoisted(() => ({ saved: [] as unknown[], disk: [] as unknown[] }))
vi.mock('#lib/app-status-store.js', () => ({
  TAG_NAME_PATTERNS_MAX: 10,
  loadTagNamePatterns: vi.fn(() => Promise.resolve(store.disk)),
  saveTagNamePatterns: vi.fn((patterns: unknown[]) => {
    store.saved = patterns
    return Promise.resolve()
  }),
}))

let cleanup: (() => void) | undefined
beforeEach(() => {
  store.saved = []
  store.disk = []
})
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const ROWS: NamePreviewRow[] = [
  {
    path: '/b/01.mp3',
    fileName: '01 - Intro.mp3',
    fields: [
      { field: 'track', value: '01' },
      { field: 'title', value: 'Intro' },
    ],
  },
  { path: '/b/x.mp3', fileName: 'x.mp3', fields: null },
]

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

async function mountPanel(rows = ROWS) {
  const tags = fakeTags(rows)
  const props = $state({ disabled: false, state: asTags(tags), runScoped: fakeRunScoped(), scopeKey: 'A' })
  const mounted = await mountInto(NamePatternPanel, props)
  cleanup = mounted.cleanup
  await settle()
  return { root: mounted.root, tags, props }
}

async function click(el: HTMLElement): Promise<void> {
  el.click()
  await settle()
}

const pattern = (root: HTMLElement) => byId<HTMLInputElement>(root, 'tag-name-pattern')

describe('NamePatternPanel', () => {
  it('previews what the pattern reads, row by row, with the scope', async () => {
    const { root, tags } = await mountPanel()
    await typeInto(pattern(root), '%track% - %title%')
    await click(buttonNamed(root, 'Preview'))
    expect(tags.previewFromNames).toHaveBeenCalledWith({ kind: 'format', pattern: '%track% - %title%' }, SCOPE)
    const rows = [...root.querySelectorAll('tbody tr')].map((tr) =>
      [...tr.querySelectorAll('td')].map((td) =>
        [...td.querySelectorAll('li')].length > 0
          ? [...td.querySelectorAll('li')].map((li) => li.textContent.replace(/\s+/g, ' ').trim()).join(' | ')
          : td.textContent.trim(),
      ),
    )
    expect(rows).toEqual([
      ['01 - Intro.mp3', 'Track 01 | Title Intro'],
      ['x.mp3', 'Doesn’t match'],
    ])
    expect(tags.tool).not.toHaveBeenCalled()
  })

  it('Take is off until a preview, runs fromFileName with the same scope, and goes off when the pattern is edited', async () => {
    const { root, tags } = await mountPanel()
    await typeInto(pattern(root), '%track% - %title%')
    expect(buttonNamed(root, 'Take').disabled).toBe(true)
    await click(buttonNamed(root, 'Preview'))
    expect(buttonNamed(root, 'Take').disabled).toBe(false)
    await typeInto(pattern(root), '%title%')
    expect(buttonNamed(root, 'Take').disabled).toBe(true)
    expect(root.textContent).toContain('Preview again to take this pattern.')
    await click(buttonNamed(root, 'Preview'))
    await click(buttonNamed(root, 'Take'))
    expect(tags.tool).toHaveBeenCalledWith(
      { kind: 'fromFileName', pattern: { kind: 'format', pattern: '%title%' } },
      SCOPE,
    )
  })

  it('switching the mode makes the preview stale and previews as a regular expression', async () => {
    const { root, tags } = await mountPanel()
    await typeInto(pattern(root), '(?<title>.+)')
    await click(buttonNamed(root, 'Preview'))
    await click(buttonNamed(root, 'Regular expression'))
    expect(buttonNamed(root, 'Take').disabled).toBe(true)
    await click(buttonNamed(root, 'Preview'))
    expect(tags.previewFromNames).toHaveBeenLastCalledWith({ kind: 'regex', pattern: '(?<title>.+)' }, SCOPE)
  })

  it('Take applies to exactly the tracks the preview listed, and a changed selection makes the preview stale', async () => {
    const { root, tags, props } = await mountPanel()
    await typeInto(pattern(root), '%title%')
    await click(buttonNamed(root, 'Preview'))
    expect(tags.previewFromNames).toHaveBeenCalledWith(expect.anything(), SCOPE)
    props.scopeKey = 'B'
    await settle()
    expect(buttonNamed(root, 'Take').disabled).toBe(true)
    expect(root.textContent).toContain('Preview again to take this pattern.')
    expect(tags.tool).not.toHaveBeenCalled()
    await click(buttonNamed(root, 'Preview'))
    await click(buttonNamed(root, 'Take'))
    expect(tags.tool).toHaveBeenCalledWith(expect.anything(), SCOPE)
  })

  it('an empty pattern cannot be previewed', async () => {
    const { root } = await mountPanel()
    expect(buttonNamed(root, 'Preview').disabled).toBe(true)
  })

  it('remembers a taken pattern, and the recent list refills the box', async () => {
    const { root } = await mountPanel()
    await typeInto(pattern(root), '%title%')
    await click(buttonNamed(root, 'Preview'))
    await click(buttonNamed(root, 'Take'))
    expect(store.saved).toEqual([{ kind: 'format', pattern: '%title%' }])
    await typeInto(pattern(root), '')
    await click(buttonNamed(root, '%title%'))
    expect(pattern(root).value).toBe('%title%')
  })

  it('lists the recent patterns saved before, with their mode', async () => {
    store.disk = [{ kind: 'regex', pattern: '(?<title>.+)' }]
    const { root, tags } = await mountPanel()
    await click(buttonNamed(root, '(?<title>.+)'))
    expect(pattern(root).value).toBe('(?<title>.+)')
    await click(buttonNamed(root, 'Preview'))
    expect(tags.previewFromNames).toHaveBeenCalledWith({ kind: 'regex', pattern: '(?<title>.+)' }, SCOPE)
  })

  it.each([
    [{ kind: 'badPattern', error: { kind: 'badRegex' } } as const, 'The pattern isn’t a valid regular expression.'],
    [{ kind: 'badPattern', error: { kind: 'unknownField', name: 'titel' } } as const, '“titel” isn’t a tag field.'],
    [
      { kind: 'badPattern', error: { kind: 'noFields' } } as const,
      'The pattern reads no tag field. Use a field like %title% or a named group like (?<title>.+).',
    ],
  ])('words the preview error %j', async (error, text) => {
    const { root, tags } = await mountPanel()
    tags.previewFromNames.mockImplementation(() => {
      tags.error = error
      return Promise.resolve({ ok: false as const, error })
    })
    await typeInto(pattern(root), 'x')
    await click(buttonNamed(root, 'Preview'))
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBe(text)
    expect(buttonNamed(root, 'Take').disabled).toBe(true)
  })

  it('words an error from Take too', async () => {
    const { root, tags } = await mountPanel()
    tags.tool.mockImplementation(() => {
      tags.error = { kind: 'badPattern', error: { kind: 'unknownField', name: 'x' } }
      return Promise.resolve()
    })
    await typeInto(pattern(root), '%x%')
    await click(buttonNamed(root, 'Preview'))
    await click(buttonNamed(root, 'Take'))
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBe('“x” isn’t a tag field.')
    expect(store.saved).toEqual([])
  })
})
