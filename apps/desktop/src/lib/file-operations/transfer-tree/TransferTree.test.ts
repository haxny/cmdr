import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import TransferTree from './TransferTree.svelte'
import { TRANSFER_TREE_FIXTURES } from './fixtures/catalog'
import { createFixturePlayer } from './fixtures/fixture-player.svelte'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

let target: HTMLElement

beforeEach(() => {
  document.body.innerHTML = ''
})

/** Lets a frame pass (a scroll's pull waits for one), then every answer land. */
async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 20))
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
    expect(track.getAttribute('aria-label')).toMatch(/^track_0001\.flac, (Moving|Done), /)
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
    expect(reports.getAttribute('aria-label')).toContain('1 failed')
    expect(rowNamed('draft.docx').querySelector('.count')?.textContent.trim()).toBe('Skipped')
    expect(rowNamed('locked.pdf').querySelector('.count .error')).not.toBeNull()
  })
})
