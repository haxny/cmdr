// @vitest-environment jsdom
/**
 * Speech-level expectations for the file list, through `@guidepup/virtual-screen-reader`.
 *
 * It models the ARIA spec over jsdom, not WebKit or VoiceOver, so it guards OUR markup: the
 * focused listbox naming its pane and folder, the cursor row named "{name} ({kind})" as the
 * cursor moves, every row's place in the whole list (`..` included), marking saying
 * "selected", and the pane announcer's message arriving. What VoiceOver really says is the
 * listening harness's job (`docs/specs/a11y-file-list-focus-plan.md`).
 *
 * The keys here stand in for the pane's: a listener moves the cursor and marks rows the way
 * `FilePane` would, because the list itself handles no keys.
 *
 * jsdom, not the suite's happy-dom: the virtual reader drives keys through
 * `@testing-library/user-event` and reads names with `dom-accessibility-api`, both written
 * against jsdom.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import { virtual } from '@guidepup/virtual-screen-reader'
import { dirEntry, fileEntry } from './test-full-list'
import { mountBriefList } from './test-brief-list'
import LiveAnnouncer from '#lib/ui/LiveAnnouncer.svelte'

vi.mock('#lib/tauri-commands/index.js', async () =>
  (await import('./test-file-list-mocks')).tauriCommandsMock({
    getBriefColumnTextWidths: () => Promise.resolve({ status: 'ok', data: { widths: [], missingCodePoints: [] } }),
  }),
)
vi.mock('#lib/icon-cache.js', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('#lib/indexing/index-state.svelte.js', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('#lib/settings/reactive-settings.svelte.js', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock({
    getBriefColumnWidthMode: () => 'paneWidth',
    getBriefColumnWidthMaxPx: () => 400,
  }),
)
vi.mock('#lib/settings/settings-store.js', async () =>
  (await import('./test-file-list-mocks')).settingsStoreMock({
    'advanced.virtualizationBufferColumns': 2,
  }),
)

/** `vo-test` from the listening harness: seven entries plus the `..` row. */
const ENTRIES = [
  dirEntry({ name: 'subfolder' }),
  fileEntry({ name: 'a.txt' }),
  fileEntry({ name: 'b.txt' }),
  fileEntry({ name: 'c.zip', isArchive: true }),
  fileEntry({ name: 'd-link', isSymlink: true }),
  fileEntry({ name: 'e.bin' }),
  fileEntry({ name: 'f.txt' }),
]

let cleanups: (() => void)[] = []

afterEach(async () => {
  for (const cleanup of cleanups) cleanup()
  cleanups = []
  await virtual.stop()
  document.body.innerHTML = ''
})

/** Lets the one-frame-late active descendant and the announcer's frame land. */
async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 150))
  await tick()
}

async function mountPane() {
  const live = $state({ cursorIndex: 0 })
  const marked = new SvelteSet<number>()
  const list = await mountBriefList({
    entries: ENTRIES,
    viewport: { clientHeight: 400, clientWidth: 800 },
    liveProps: live,
    props: {
      hasParent: true,
      parentPath: '/Users/me',
      currentPath: '/Users/me/vo-test',
      totalCount: ENTRIES.length + 1,
      selectedIndices: marked,
      listLabel: 'Left pane, vo-test',
    },
  })
  await list.settle(() => list.rows().length === ENTRIES.length + 1, 'every vo-test row')

  // The pane's keys: ↓ moves the cursor, Space marks the cursor row and moves on.
  const onKey = (event: KeyboardEvent): void => {
    if (event.key === 'ArrowDown') live.cursorIndex = Math.min(live.cursorIndex + 1, ENTRIES.length)
    if (event.key === 'ArrowUp') live.cursorIndex = Math.max(live.cursorIndex - 1, 0)
    if (event.key === ' ') {
      if (marked.has(live.cursorIndex)) marked.delete(live.cursorIndex)
      else marked.add(live.cursorIndex)
    }
  }
  document.addEventListener('keydown', onKey)
  cleanups.push(() => {
    document.removeEventListener('keydown', onKey)
  })

  await virtual.start({ container: document.body })
  list.component.focusList()
  await settle()
  return list
}

async function press(key: string): Promise<string> {
  await virtual.press(key)
  await settle()
  return virtual.lastSpokenPhrase()
}

/**
 * The next `count` options, read with the reader's own cursor (VO-→), with role and state. The
 * walk also stops on each option's text and its "end of" marker; only the options count.
 */
async function walkOptions(count: number): Promise<string[]> {
  const phrases: string[] = []
  for (let step = 0; step < 100 && phrases.length < count; step++) {
    await virtual.next()
    const phrase = await virtual.lastSpokenPhrase()
    if (phrase.startsWith('option, ')) phrases.push(phrase)
  }
  return phrases
}

describe('the file list, spoken', () => {
  // The virtual reader says the active descendant as "active descendant {name}" on the
  // focused list's phrase, and only after a key, so the row arrives with the first ↓.
  it('names the focused list after its pane and folder, and each ↓ names the new cursor row', async () => {
    await mountPane()
    expect(await virtual.lastSpokenPhrase()).toBe(
      'listbox, Left pane, vo-test, orientated vertically, multi-selectable',
    )

    expect(await press('ArrowDown')).toBe(
      'listbox, Left pane, vo-test, orientated vertically, active descendant subfolder (folder), multi-selectable',
    )
    expect(await press('ArrowDown')).toBe(
      'listbox, Left pane, vo-test, orientated vertically, active descendant a.txt, multi-selectable',
    )
  })

  it('places every row in the whole list, `..` included, and says which rows are marked', async () => {
    await mountPane()
    await press('ArrowDown')
    await press('ArrowDown')
    await press(' ')

    expect(await walkOptions(5)).toEqual([
      'option, Parent folder, not selected, position 1, set size 8',
      'option, subfolder (folder), not selected, position 2, set size 8',
      'option, a.txt, selected, position 3, set size 8',
      'option, b.txt, not selected, position 4, set size 8',
      'option, c.zip (archive), not selected, position 5, set size 8',
    ])
  })

  it('speaks the pane announcer’s message as a polite announcement', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const announcer = mount(LiveAnnouncer, { target, props: { 'data-pane-announcer': true } })
    cleanups.push(() => {
      void unmount(announcer)
    })
    await virtual.start({ container: document.body })
    ;(announcer as unknown as { announce: (message: string) => void }).announce('Left pane, vo-test')
    await settle()
    expect(await virtual.spokenPhraseLog()).toContain('polite: Left pane, vo-test')
  })
})
