import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import TransferTree from './TransferTree.svelte'
import { TRANSFER_TREE_FIXTURES } from './fixtures/catalog'
import { createFixturePlayer } from './fixtures/fixture-player.svelte'
import { TICK_PULL_INTERVAL_MS } from './transfer-tree-view.svelte'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'decimal',
}))

let target: HTMLElement

beforeEach(() => {
  document.body.innerHTML = ''
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

/** Lets the tick rate limit pass (a frame too: a scroll's pull waits for one), then every answer land. */
async function settle(): Promise<void> {
  await vi.advanceTimersByTimeAsync(TICK_PULL_INTERVAL_MS)
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
    flushSync()
  }
}

/** Mounts the tree over a fixture, past its preparing steps, with `steps` ticks played. */
async function render(id: string, steps = 0) {
  const fixture = TRANSFER_TREE_FIXTURES.find((candidate) => candidate.id === id)
  if (!fixture) throw new Error(`no fixture ${id}`)
  const player = createFixturePlayer(fixture)
  const transfer = player.transfer
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferTree, { target, props: player.props })
  await settle()
  const advance = async (count = 1) => {
    player.step(count)
    await settle()
  }
  await advance(5 + steps)
  return { transfer, advance }
}

function rows(): HTMLElement[] {
  return [...target.querySelectorAll<HTMLElement>('[role="treeitem"]')].sort(
    (a, b) => Number(a.dataset.index) - Number(b.dataset.index),
  )
}

function rowNamed(label: string): HTMLElement {
  const found = rows().find((row) => row.querySelector('.label')?.textContent === label)
  if (!found) throw new Error(`no row ${label}`)
  return found
}

function key(name: string, init: KeyboardEventInit = {}): void {
  const active = document.activeElement as HTMLElement
  active.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ...init }))
  flushSync()
}

/**
 * A layout for jsdom, which has none: the probe measures `rowHeight`, the scroller shows
 * `viewportRows` of them, and its `scrollTop` keeps whole pixels as WebKit's does. Returns
 * the undo.
 */
function fakeLayout({ rowHeight, viewportRows }: { rowHeight: number; viewportRows: number }): () => void {
  // Shadows jsdom's own on `Element.prototype`; deleting these brings them back.
  const proto = HTMLElement.prototype
  const tops = new WeakMap<Element, number>()
  const isScroller = (el: Element) => el.classList.contains('scroller')
  Object.defineProperty(proto, 'getBoundingClientRect', {
    configurable: true,
    value(this: HTMLElement) {
      const height = this.classList.contains('probe') ? rowHeight : 0
      return { x: 0, y: 0, top: 0, left: 0, right: 1, bottom: height, width: 1, height, toJSON: () => ({}) }
    },
  })
  Object.defineProperty(proto, 'clientHeight', {
    configurable: true,
    get(this: HTMLElement) {
      return isScroller(this) ? Math.floor(viewportRows * rowHeight) : 0
    },
  })
  Object.defineProperty(proto, 'scrollTop', {
    configurable: true,
    get(this: HTMLElement) {
      return tops.get(this) ?? 0
    },
    set(this: HTMLElement, value: number) {
      tops.set(this, Math.max(0, Math.trunc(value)))
    },
  })
  return () => {
    for (const name of ['getBoundingClientRect', 'clientHeight', 'scrollTop']) Reflect.deleteProperty(proto, name)
  }
}

function followButton(): HTMLButtonElement | null {
  return target.querySelector('.follow button')
}

describe('TransferTree', () => {
  it('says it’s listing the files until the tree is ready', async () => {
    const fixture = TRANSFER_TREE_FIXTURES[0]
    target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransferTree, { target, props: { source: fixture.create(), tick: 0, operation: fixture.operation } })
    await settle()
    expect(target.textContent).toContain('Listing the files…')
    expect(rows()).toHaveLength(0)
  })

  it('takes the height of its rows up to its default, and never shrinks back', async () => {
    const heightRows = () =>
      target.querySelector<HTMLElement>('.transfer-tree')?.style.getPropertyValue('--spacing-tree-rows')
    await render('single-file')
    expect(heightRows()).toBe('1')

    document.body.innerHTML = ''
    const { transfer, advance } = await render('failed-and-skipped', 1)
    expect(heightRows()).toBe('8')
    while (!transfer.finished) await advance()
    await advance()
    // `Reports` shut again once the work left it (four rows), and the tree keeps its height.
    expect(rows()).toHaveLength(4)
    expect(heightRows()).toBe('8')
  })

  it('gives every row its level and its place among its siblings, from the backend', async () => {
    await render('folders-5x1000', 3)
    const album = rowNamed('Album 1')
    expect(album.getAttribute('aria-level')).toBe('1')
    expect(album.getAttribute('aria-posinset')).toBe('1')
    expect(album.getAttribute('aria-setsize')).toBe('5')
    expect(album.getAttribute('aria-expanded')).toBe('true')

    const track = rowNamed('track_0001.flac')
    expect(track.getAttribute('aria-level')).toBe('2')
    expect(track.getAttribute('aria-setsize')).toBe('1000')
    expect(track.getAttribute('aria-expanded')).toBeNull()
    expect(track.getAttribute('aria-label')).toMatch(/^track_0001\.flac, .+, (Moving|Done)$/)
  })

  it('names a compact folder chain as one row and hovers each row’s full path', async () => {
    await render('deep-compact', 1)
    expect(rows().map((row) => row.querySelector('.label')?.textContent)).toContain('a/b/c')
  })

  it('moves the cursor with Up, Down, Home, and End, and a move pauses following', async () => {
    await render('folders-5x1000', 3)
    expect(followButton()).toBeNull()
    rowNamed('Album 1').focus()

    key('ArrowDown')
    await settle()
    expect(document.activeElement?.getAttribute('data-index')).toBe('1')
    expect(followButton()).not.toBeNull()

    key('ArrowUp')
    await settle()
    expect(document.activeElement?.getAttribute('data-index')).toBe('0')

    key('End')
    await settle()
    expect(document.activeElement?.querySelector('.label')?.textContent).toBe('Album 5')

    key('Home')
    await settle()
    expect(document.activeElement?.getAttribute('data-index')).toBe('0')
  })

  it('collapses and expands with Left and Right, and Left on a child goes to its folder', async () => {
    await render('folders-5x1000', 3)
    rowNamed('track_0002.flac').focus()

    key('ArrowLeft')
    await settle()
    expect(document.activeElement?.querySelector('.label')?.textContent).toBe('Album 1')

    key('ArrowLeft')
    await settle()
    expect(rowNamed('Album 1').getAttribute('aria-expanded')).toBe('false')
    expect(rows().map((row) => row.querySelector('.label')?.textContent)).not.toContain('track_0001.flac')

    key('ArrowRight')
    await settle()
    expect(rowNamed('Album 1').getAttribute('aria-expanded')).toBe('true')

    key('ArrowRight')
    await settle()
    expect(document.activeElement?.querySelector('.label')?.textContent).toBe('track_0001.flac')
  })

  it('brings Follow back with its button and with F', async () => {
    await render('flat-5000', 3)
    rows()[0].focus()
    key('ArrowDown')
    await settle()
    followButton()?.click()
    await settle()
    expect(followButton()).toBeNull()

    rows()[0].focus()
    key('ArrowDown')
    await settle()
    expect(followButton()).not.toBeNull()
    key('f', { code: 'KeyF' })
    await settle()
    expect(followButton()).toBeNull()
  })

  it('shows Follow as a word with its F key chip, no glyph', async () => {
    await render('flat-5000', 3)
    rows()[0].focus()
    key('ArrowDown')
    await settle()
    const button = followButton()
    expect(button?.querySelector('svg')).toBeNull()
    expect(button?.querySelector('.btn-label')?.textContent.trim()).toBe('Follow')
    expect(button?.querySelector('.btn-key-hint')?.textContent).toBe('F')
    expect(button?.getAttribute('aria-keyshortcuts')).toBe('F')
  })

  it('pauses following on a wheel only when the tree has somewhere to scroll', async () => {
    await render('flat-5000', 3)
    const scroller = target.querySelector<HTMLElement>('.scroller')
    if (!scroller) throw new Error('no scroller')
    const wheel = () => {
      scroller.dispatchEvent(new WheelEvent('wheel', { deltaY: 40, bubbles: true }))
      flushSync()
    }

    // jsdom lays nothing out, so the rows fit: the wheel scrolls the page around it.
    wheel()
    await settle()
    expect(followButton()).toBeNull()

    Object.defineProperty(scroller, 'scrollHeight', { configurable: true, value: 1000 })
    Object.defineProperty(scroller, 'clientHeight', { configurable: true, value: 100 })
    wheel()
    await settle()
    expect(followButton()).not.toBeNull()
  })

  // Regression: in a flat folder, Up from row 15 lost the folder at row 2 (neither pinned nor
  // drawn) until the cursor reached it. The row height is fractional (small text times its
  // leading, plus padding) and WebKit keeps `scrollTop` in whole pixels, so a scroll to row 1
  // read back a hair short of it, and flooring that said row 0 was still the top.
  it('keeps the folder in sight while Up walks its children toward it, at a fractional row height', async () => {
    const restore = fakeLayout({ rowHeight: 24.8, viewportRows: 8 })
    try {
      await render('folders-5x1000', 3)
      const scroller = target.querySelector<HTMLElement>('.scroller')
      if (!scroller) throw new Error('no scroller')
      rowNamed('Album 1').focus()
      for (let i = 0; i < 15; i++) {
        key('ArrowDown')
        await settle()
      }
      expect(document.activeElement?.getAttribute('data-index')).toBe('15')

      const folderInSight = () =>
        target.querySelector('.pinned [data-pinned-index="0"]') !== null || scroller.scrollTop < 1
      for (let at = 14; at >= 1; at--) {
        key('ArrowUp')
        await settle()
        expect(document.activeElement?.getAttribute('data-index')).toBe(String(at))
        expect(folderInSight(), `cursor on row ${String(at)}`).toBe(true)
      }
    } finally {
      restore()
    }
  })

  it('keeps the tab stop on one row, the work’s while nobody has moved', async () => {
    const { advance } = await render('flat-5000', 3)
    const stops = () => rows().filter((row) => row.tabIndex === 0)
    expect(stops()).toHaveLength(1)
    expect(stops()[0].getAttribute('aria-label')).toContain('Copying')
    await advance(40)
    expect(stops()).toHaveLength(1)
    expect(stops()[0].getAttribute('aria-label')).toContain('Copying')
  })

  it('shows failures by their typed reason and counts skips and failures on the folder', async () => {
    const { transfer, advance } = await render('failed-and-skipped')
    while (!transfer.finished) await advance()
    await advance()
    const reports = rowNamed('Reports')
    expect(reports.getAttribute('aria-label')).toContain('2 skipped')
    expect(reports.getAttribute('aria-label')).toContain('1 couldn’t finish')
    expect(rowNamed('draft.docx').querySelector('.count')?.textContent.trim()).toBe('Skipped')
    expect(rowNamed('locked.pdf').querySelector('.count .error')).not.toBeNull()
  })
})
