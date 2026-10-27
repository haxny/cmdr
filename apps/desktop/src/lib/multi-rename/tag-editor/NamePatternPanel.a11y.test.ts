/** Tier 3 a11y tests for `NamePatternPanel.svelte`: empty, with recent patterns, with a preview, with an error, locked. */
import { afterEach, describe, it, vi } from 'vitest'
import { tick } from 'svelte'
import NamePatternPanel from './NamePatternPanel.svelte'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { asTags, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

const store = vi.hoisted(() => ({ disk: [] as unknown[] }))
vi.mock('#lib/app-status-store.js', () => ({
  TAG_NAME_PATTERNS_MAX: 10,
  loadTagNamePatterns: vi.fn(() => Promise.resolve(store.disk)),
  saveTagNamePatterns: vi.fn(() => Promise.resolve()),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  store.disk = []
  document.body.innerHTML = ''
})

const ROWS = [
  { path: '/b/1.mp3', fileName: '01 - Intro.mp3', fields: [{ field: 'title' as const, value: 'Intro' }] },
  { path: '/b/2.mp3', fileName: 'x.mp3', fields: null },
]

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

async function mountPanel(disabled = false) {
  const tags = fakeTags(ROWS)
  const mounted = await mountInto(NamePatternPanel, {
    disabled,
    state: asTags(tags),
    runScoped: fakeRunScoped(),
    scopeKey: 'k',
  })
  cleanup = mounted.cleanup
  await settle()
  return { root: mounted.root, tags }
}

async function preview(root: HTMLElement, text: string): Promise<void> {
  await typeInto(byId<HTMLInputElement>(root, 'tag-name-pattern'), text)
  ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Preview')?.click()
  await settle()
}

describe('NamePatternPanel a11y', () => {
  it('empty has no violations', async () => {
    await expectNoA11yViolations((await mountPanel()).root)
  })

  it('with recent patterns has no violations', async () => {
    store.disk = [
      { kind: 'format', pattern: '%track% - %title%' },
      { kind: 'regex', pattern: '(?<title>.+)' },
    ]
    await expectNoA11yViolations((await mountPanel()).root)
  })

  it('with a preview table has no violations', async () => {
    const { root } = await mountPanel()
    await preview(root, '%title%')
    await expectNoA11yViolations(root)
  })

  it('with a pattern error has no violations', async () => {
    const { root, tags } = await mountPanel()
    tags.previewFromNames.mockImplementation(() => {
      const error = { kind: 'badPattern', error: { kind: 'unknownField', name: 'titel' } } as const
      tags.error = error
      return Promise.resolve({ ok: false as const, error })
    })
    await preview(root, '%titel%')
    await expectNoA11yViolations(root)
  })

  it('locked has no violations', async () => {
    await expectNoA11yViolations((await mountPanel(true)).root)
  })
})
