/**
 * The cover section: every distinct cover as a thumbnail with how many files carry it and its size, the files without
 * one counted too; a click opens it large; a pasted picture or a dropped file becomes the cover of every file; the
 * folder picker and Remove stay; read-only locks it all.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { CoverChoice, CoverSummary } from '#lib/ipc/bindings.js'
import CoverPanel from './CoverPanel.svelte'
import type { CoverDropEvent } from './cover-drop'
import { mountInto } from './tag-cell.test-helper'
import { mount, tick, unmount, type ComponentProps } from 'svelte'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const SAME: CoverSummary = { kind: 'same', mime: 'image/jpeg', bytes: 2048, width: 600, height: 400, key: 'k1' }
const FRONT: CoverChoice = {
  key: 'k1',
  mime: 'image/jpeg',
  bytes: 2048,
  width: 600,
  height: 400,
  files: 12,
  nearDuplicates: 0,
}
const BACK: CoverChoice = {
  key: 'k2',
  mime: 'image/png',
  bytes: 4096,
  width: null,
  height: null,
  files: 1,
  nearDuplicates: 0,
}
const NONE: CoverChoice = { key: 'none', mime: '', bytes: 0, width: null, height: null, files: 3, nearDuplicates: 0 }
const URLS: Record<string, string> = { k1: 'data:image/jpeg;base64,/9j/', k2: 'data:image/png;base64,iVBORw0KGgo=' }

type DropHandler = (event: CoverDropEvent) => void

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

function props(over: Partial<ComponentProps<typeof CoverPanel>> & { choices?: CoverChoice[] | null } = {}) {
  const { choices, ...rest } = over
  return {
    cover: { kind: 'multiple' } as CoverSummary,
    editable: true,
    revision: 1,
    listChoices: vi.fn(() => Promise.resolve(choices === undefined ? [FRONT, BACK, NONE] : choices)),
    loadThumbnail: vi.fn((key: string) => Promise.resolve(URLS[key] ?? null)),
    listCandidates: vi.fn(() => Promise.resolve<{ fileName: string; size: number }[] | null>([])),
    onView: vi.fn(),
    onPick: vi.fn(),
    onEdit: vi.fn(),
    onPaste: vi.fn(),
    onDrop: vi.fn(),
    onUseForAll: vi.fn(),
    subscribeDrop: vi.fn(() => Promise.resolve(() => {})),
    ...rest,
  }
}

async function mountPanel(over: Parameters<typeof props>[0] = {}) {
  const given = props(over)
  const mounted = await mountInto(CoverPanel, given)
  cleanup = mounted.cleanup
  await settle()
  return { root: mounted.root, ...given }
}

function button(root: HTMLElement, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll('button')].find((b) => b.textContent.trim().startsWith(text))
  if (!found) throw new Error(`no ${text} button`)
  return found
}

function tiles(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('.choice')]
}

/** A paste of `items` while focus is in the cover section. */
function paste(root: HTMLElement, items: { type: string; file: File | null }[]): Event {
  root.querySelector<HTMLElement>('section.cover')?.focus()
  const event = new Event('paste', { bubbles: true, cancelable: true })
  Object.defineProperty(event, 'clipboardData', {
    value: { items: items.map(({ type, file }) => ({ kind: 'file', type, getAsFile: () => file })) },
  })
  ;(document.activeElement ?? document.body).dispatchEvent(event)
  return event
}

describe('CoverPanel', () => {
  it('shows a thumbnail per cover with how many files carry it and its size, and counts the files without one', async () => {
    const { root, loadThumbnail } = await mountPanel()
    const shown = tiles(root)
    expect(shown.map((tile) => tile.querySelector('.caption')?.textContent.trim())).toEqual([
      '12 files · 2.00 KiB',
      '1 file · 4.00 KiB',
      '3 files',
    ])
    expect(shown[0].querySelector('img')?.getAttribute('src')).toBe(URLS.k1)
    expect(shown[1].querySelector('img')?.getAttribute('src')).toBe(URLS.k2)
    expect(shown[2].textContent).toContain('No cover')
    expect(shown[2].querySelector('img')).toBeNull()
    expect(loadThumbnail).toHaveBeenCalledTimes(2)
    expect(loadThumbnail).not.toHaveBeenCalledWith('none')
  })

  it('a tile whose picture didn’t come is asked once more, then says there is no preview', async () => {
    const answers: Partial<Record<string, (string | null)[]>> = { k1: [null, URLS.k1], k2: [null, null] }
    const { root, loadThumbnail } = await mountPanel({
      loadThumbnail: vi.fn((key: string) => Promise.resolve(answers[key]?.shift() ?? null)),
    })
    await settle()
    const shown = tiles(root)
    expect(shown[0].querySelector('img')?.getAttribute('src')).toBe(URLS.k1)
    expect(shown[1].querySelector('img')).toBeNull()
    expect(shown[1].querySelector('.frame')?.textContent.trim()).toBe('No preview')
    expect(loadThumbnail).toHaveBeenCalledTimes(4)
  })

  it('names each cover for a screen reader', async () => {
    const { root } = await mountPanel()
    expect(tiles(root)[0].getAttribute('aria-label')).toBe(
      '12 files · 2.00 KiB: open the cover (image/jpeg, 600×400 px, 2.00 KiB)',
    )
  })

  it('a click on a cover opens it large, with its image', async () => {
    const { root, onView } = await mountPanel()
    tiles(root)[1].click()
    expect(onView).toHaveBeenCalledWith({ choice: BACK, total: 16 })
  })

  it('a new view lists the choices again; an image already loaded is not asked for again', async () => {
    const given = props()
    const live = $state<ComponentProps<typeof CoverPanel>>(given)
    const root = document.createElement('div')
    document.body.appendChild(root)
    const panel = mount(CoverPanel, { target: root, props: live })
    cleanup = () => {
      void unmount(panel)
      root.remove()
    }
    await settle()
    vi.mocked(given.listChoices).mockResolvedValue([{ ...FRONT, files: 16 }])
    live.revision = 2
    await settle()
    expect(given.listChoices).toHaveBeenCalledTimes(2)
    expect(tiles(root)).toHaveLength(1)
    expect(given.loadThumbnail).toHaveBeenCalledTimes(2)
  })

  it('a late answer for an older view is dropped', async () => {
    const answers: ((choices: CoverChoice[]) => void)[] = []
    const given = props({
      listChoices: vi.fn(() => new Promise<CoverChoice[] | null>((resolve) => answers.push(resolve))),
    })
    const live = $state<ComponentProps<typeof CoverPanel>>(given)
    const root = document.createElement('div')
    document.body.appendChild(root)
    const panel = mount(CoverPanel, { target: root, props: live })
    cleanup = () => {
      void unmount(panel)
      root.remove()
    }
    await settle()
    live.revision = 2
    await settle()
    answers[1]([FRONT])
    await settle()
    answers[0]([FRONT, BACK, NONE])
    await settle()
    expect(tiles(root)).toHaveLength(1)
  })

  it('a new view clears the last paste or drop notice', async () => {
    const given = props()
    const live = $state<ComponentProps<typeof CoverPanel>>(given)
    const root = document.createElement('div')
    document.body.appendChild(root)
    const panel = mount(CoverPanel, { target: root, props: live })
    cleanup = () => {
      void unmount(panel)
      root.remove()
    }
    await settle()
    paste(root, [{ type: 'text/plain', file: null }])
    await settle()
    expect(root.querySelector('[role="status"]')).not.toBeNull()
    live.revision = 2
    await settle()
    expect(root.querySelector('[role="status"]')).toBeNull()
  })

  describe('covers that look the same', () => {
    const ALIKE: CoverChoice = { ...FRONT, files: 50, nearDuplicates: 49 }

    it('when one look covers every file, says so and offers it for all, as one edit', async () => {
      const { root, onUseForAll } = await mountPanel({ choices: [ALIKE] })
      expect(tiles(root)).toHaveLength(1)
      expect(root.querySelector('.alike p')?.textContent.trim()).toBe('50 covers look almost the same')
      button(root, 'Use it for all 50 files').click()
      expect(onUseForAll).toHaveBeenCalledTimes(1)
      expect(onUseForAll).toHaveBeenCalledWith('k1')
    })

    it('the user’s book: one look on some files and none on the rest still offers it for every file', async () => {
      const some: CoverChoice = { ...FRONT, files: 24, nearDuplicates: 23 }
      const { root, onUseForAll } = await mountPanel({ choices: [some, { ...NONE, files: 26 }] })
      expect(root.querySelector('.alike p')?.textContent.trim()).toBe(
        '24 covers look almost the same; 26 files have none',
      )
      button(root, 'Use it for all 50 files').click()
      expect(onUseForAll).toHaveBeenCalledWith('k1')
    })

    it('says nothing when the files share one cover by bytes, or the covers differ', async () => {
      for (const choices of [[FRONT], [FRONT, NONE], [ALIKE, BACK], [ALIKE, BACK, NONE]]) {
        const { root } = await mountPanel({ choices })
        expect(root.textContent).not.toContain('look almost the same')
        cleanup?.()
        cleanup = undefined
      }
    })

    it('the offer is off when read-only', async () => {
      const { root } = await mountPanel({ choices: [ALIKE], editable: false })
      expect(button(root, 'Use it for all 50 files').disabled).toBe(true)
    })
  })

  describe('many covers', () => {
    const many: CoverChoice[] = Array.from({ length: 11 }, (_, i) => ({ ...BACK, key: `m${String(i)}` }))

    it('shows eight tiles and "3 more", a toggle that keeps focus; the files without a cover always show', async () => {
      const { root } = await mountPanel({ choices: [...many, NONE] })
      expect(tiles(root)).toHaveLength(9)
      expect(tiles(root).at(-1)?.textContent).toContain('No cover')
      const more = button(root, '3 more')
      expect(more.getAttribute('aria-expanded')).toBe('false')
      more.focus()
      more.click()
      await settle()
      expect(tiles(root)).toHaveLength(12)
      expect(more.isConnected).toBe(true)
      expect(document.activeElement).toBe(more)
      expect(more.getAttribute('aria-expanded')).toBe('true')
      expect(more.textContent.trim()).toBe('Show fewer')
      more.click()
      await settle()
      expect(tiles(root)).toHaveLength(9)
      expect(more.getAttribute('aria-expanded')).toBe('false')
      expect(more.textContent.trim()).toBe('3 more')
    })

    it('eight covers show without a "more"', async () => {
      const { root } = await mountPanel({ choices: many.slice(0, 8) })
      expect(tiles(root)).toHaveLength(8)
      expect(root.textContent).not.toContain('more')
    })
  })

  it('says so when the covers couldn’t be listed', async () => {
    const { root } = await mountPanel({ choices: null })
    expect(root.textContent).toContain('Couldn’t read the covers')
  })

  describe('paste', () => {
    it('a pasted picture goes to every file as bytes', async () => {
      const { root, onPaste } = await mountPanel()
      const event = paste(root, [
        { type: 'text/plain', file: null },
        {
          type: 'image/png',
          file: new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'image.png', { type: 'image/png' }),
        },
      ])
      await settle()
      expect(event.defaultPrevented).toBe(true)
      expect(onPaste).toHaveBeenCalledWith(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))
    })

    it('a paste with no picture says what to do', async () => {
      const { root, onPaste } = await mountPanel()
      paste(root, [{ type: 'text/plain', file: null }])
      await settle()
      expect(onPaste).not.toHaveBeenCalled()
      expect(root.querySelector('[role="status"]')?.textContent).toContain('There’s no picture on the clipboard')
    })

    it('a picture over 10 MiB is refused before it is sent', async () => {
      const { root, onPaste } = await mountPanel()
      const big = new File([new Uint8Array(10 * 1024 * 1024 + 1)], 'big.png', { type: 'image/png' })
      paste(root, [{ type: 'image/png', file: big }])
      await settle()
      expect(onPaste).not.toHaveBeenCalled()
      expect(root.querySelector('[role="status"]')?.textContent).toContain('too large')
    })

    it('a paste while focus is elsewhere, or read-only, is not the cover’s', async () => {
      const outside = document.createElement('input')
      document.body.appendChild(outside)
      const { onPaste } = await mountPanel()
      outside.focus()
      const event = new Event('paste', { bubbles: true, cancelable: true })
      outside.dispatchEvent(event)
      expect(event.defaultPrevented).toBe(false)
      outside.remove()
      cleanup?.()
      const locked = await mountPanel({ editable: false })
      paste(locked.root, [{ type: 'image/png', file: new File([new Uint8Array([1])], 'x.png') }])
      await settle()
      expect(onPaste).not.toHaveBeenCalled()
      expect(locked.onPaste).not.toHaveBeenCalled()
    })

    it('a TIFF up to 40 MiB is sent (the backend converts it); a JPEG over 10 MiB is not', async () => {
      const { root, onPaste } = await mountPanel()
      const tiff = new File([new Uint8Array(11 * 1024 * 1024)], 'image.tiff', { type: 'image/tiff' })
      paste(root, [{ type: 'image/tiff', file: tiff }])
      await settle()
      expect(onPaste).toHaveBeenCalledTimes(1)
      const jpeg = new File([new Uint8Array(11 * 1024 * 1024)], 'image.jpg', { type: 'image/jpeg' })
      paste(root, [{ type: 'image/jpeg', file: jpeg }])
      await settle()
      expect(onPaste).toHaveBeenCalledTimes(1)
    })

    it('the Paste picture button reads the clipboard: a picture goes to every file', async () => {
      const picture = new Blob([new Uint8Array([0xff, 0xd8, 0xff])], { type: 'image/jpeg' })
      const read = vi.fn(() =>
        Promise.resolve([{ types: ['text/plain', 'image/jpeg'], getType: () => Promise.resolve(picture) }]),
      )
      Object.defineProperty(navigator, 'clipboard', { value: { read }, configurable: true })
      const { root, onPaste } = await mountPanel()
      button(root, 'Paste picture').click()
      await settle()
      expect(onPaste).toHaveBeenCalledWith(new Uint8Array([0xff, 0xd8, 0xff]))
    })

    it('the Paste picture button says so with no picture, or when the clipboard can’t be read', async () => {
      Object.defineProperty(navigator, 'clipboard', {
        value: {
          read: () => Promise.resolve([{ types: ['text/plain'], getType: () => Promise.reject(new Error('x')) }]),
        },
        configurable: true,
      })
      const empty = await mountPanel()
      button(empty.root, 'Paste picture').click()
      await settle()
      expect(empty.root.querySelector('[role="status"]')?.textContent).toContain('There’s no picture on the clipboard')
      cleanup?.()
      Object.defineProperty(navigator, 'clipboard', {
        value: { read: () => Promise.reject(new DOMException('denied', 'NotAllowedError')) },
        configurable: true,
      })
      const denied = await mountPanel()
      button(denied.root, 'Paste picture').click()
      await settle()
      expect(denied.root.querySelector('[role="status"]')?.textContent).toContain('Couldn’t read the clipboard')
      expect(denied.onPaste).not.toHaveBeenCalled()
    })

    it('lets WebKit offer Paste while the cover has focus (beforepaste)', async () => {
      const { root } = await mountPanel()
      root.querySelector<HTMLElement>('section.cover')?.focus()
      const event = new Event('beforepaste', { bubbles: true, cancelable: true })
      document.body.dispatchEvent(event)
      expect(event.defaultPrevented).toBe(true)
    })
  })

  describe('drop', () => {
    async function withDrop(over: Parameters<typeof props>[0] = {}) {
      let handler: DropHandler = () => {}
      const mounted = await mountPanel({
        ...over,
        subscribeDrop: vi.fn((h: DropHandler) => {
          handler = h
          return Promise.resolve(() => {})
        }),
      })
      const section = mounted.root.querySelector<HTMLElement>('section.cover')
      if (!section) throw new Error('no section')
      section.getBoundingClientRect = () => ({ left: 0, top: 0, right: 100, bottom: 100 }) as DOMRect
      return {
        ...mounted,
        drop: (event: CoverDropEvent) => {
          handler(event)
        },
      }
    }

    it('a file dropped on the cover goes to every file by its path', async () => {
      const { root, drop, onDrop } = await withDrop()
      drop({ type: 'over', position: { x: 10, y: 10 } })
      await settle()
      expect(root.querySelector('section.cover')?.classList.contains('drop-target')).toBe(true)
      drop({ type: 'drop', paths: ['/pics/front.jpg'], position: { x: 10, y: 10 } })
      await settle()
      expect(onDrop).toHaveBeenCalledWith('/pics/front.jpg')
      expect(root.querySelector('section.cover')?.classList.contains('drop-target')).toBe(false)
    })

    it('a drop elsewhere in the window is not the cover’s; two files ask for one', async () => {
      const { root, drop, onDrop } = await withDrop()
      drop({ type: 'drop', paths: ['/pics/front.jpg'], position: { x: 500, y: 500 } })
      drop({ type: 'drop', paths: ['/a.jpg', '/b.jpg'], position: { x: 10, y: 10 } })
      await settle()
      expect(onDrop).not.toHaveBeenCalled()
      expect(root.querySelector('[role="status"]')?.textContent).toContain('Drop one picture')
    })

    it('a drop where the section is scrolled out of view is not the cover’s', async () => {
      // The section sits in a scroller that shows only its top 40 px; below that are the strip, status line and bar.
      const clip = document.createElement('div')
      clip.setAttribute('data-cover-clip', '')
      clip.getBoundingClientRect = () => ({ left: 0, top: 0, right: 100, bottom: 40 }) as DOMRect
      const { root, drop, onDrop } = await withDrop()
      const section = root.querySelector('section.cover')
      if (!section) throw new Error('no section')
      root.replaceWith(clip)
      clip.appendChild(root)
      drop({ type: 'over', position: { x: 10, y: 80 } })
      await settle()
      expect(section.classList.contains('drop-target')).toBe(false)
      drop({ type: 'drop', paths: ['/pics/front.jpg'], position: { x: 10, y: 80 } })
      await settle()
      expect(onDrop).not.toHaveBeenCalled()
      drop({ type: 'drop', paths: ['/pics/front.jpg'], position: { x: 10, y: 20 } })
      await settle()
      expect(onDrop).toHaveBeenCalledWith('/pics/front.jpg')
      clip.remove()
    })

    it('read-only takes no drop', async () => {
      const { drop, onDrop } = await withDrop({ editable: false })
      drop({ type: 'drop', paths: ['/pics/front.jpg'], position: { x: 10, y: 10 } })
      await settle()
      expect(onDrop).not.toHaveBeenCalled()
    })
  })

  describe('the folder pictures (as before)', () => {
    it('lists the folder and a pick goes to every file', async () => {
      const { root, listCandidates, onPick } = await mountPanel({
        listCandidates: vi.fn(() =>
          Promise.resolve<{ fileName: string; size: number }[] | null>([
            { fileName: 'cover.jpg', size: 1024 },
            { fileName: 'back.png', size: 4096 },
          ]),
        ),
      })
      button(root, 'From folder').click()
      await settle()
      expect(listCandidates).toHaveBeenCalledTimes(1)
      const items = [...root.querySelectorAll<HTMLButtonElement>('.candidate')]
      expect(items.map((i) => i.textContent.trim())).toEqual(['cover.jpg (1.00 KiB)', 'back.png (4.00 KiB)'])
      items[1].click()
      await settle()
      expect(onPick).toHaveBeenCalledWith('back.png')
      expect(root.querySelector('.candidates')).toBeNull()
    })

    it('says so when the folder has no pictures, and when listing failed', async () => {
      const empty = await mountPanel()
      button(empty.root, 'From folder').click()
      await settle()
      expect(empty.root.textContent).toContain('No pictures (jpg, png, webp) in this folder.')
      cleanup?.()
      const failed = await mountPanel({ listCandidates: vi.fn(() => Promise.resolve(null)) })
      button(failed.root, 'From folder').click()
      await settle()
      expect(failed.root.querySelector('[role="alert"]')?.textContent).toContain('Couldn’t list the pictures')
    })

    it('a second press while the first listing is out closes the picker for good', async () => {
      let answer: (list: { fileName: string; size: number }[]) => void = () => {}
      const { root } = await mountPanel({
        listCandidates: vi.fn(
          () =>
            new Promise<{ fileName: string; size: number }[] | null>((resolve) => {
              answer = resolve
            }),
        ),
      })
      button(root, 'From folder').click()
      await settle()
      button(root, 'From folder').click()
      await settle()
      answer([{ fileName: 'late.jpg', size: 1 }])
      await settle()
      expect(root.querySelector('.candidates')).toBeNull()
    })

    it('Remove takes every cover out; with none it is off', async () => {
      const { root, onEdit } = await mountPanel({ cover: SAME })
      button(root, 'Remove').click()
      expect(onEdit).toHaveBeenCalledWith([{ kind: 'cover', edit: 'remove' }])
      cleanup?.()
      const none = await mountPanel({ cover: { kind: 'empty' }, choices: [NONE] })
      expect(button(none.root, 'Remove').disabled).toBe(true)
    })

    it('read-only locks the folder picker and Remove, and still shows the covers', async () => {
      const { root, listCandidates, onView } = await mountPanel({ editable: false })
      expect(button(root, 'From folder').disabled).toBe(true)
      expect(button(root, 'Remove').disabled).toBe(true)
      button(root, 'From folder').click()
      expect(listCandidates).not.toHaveBeenCalled()
      tiles(root)[0].click()
      expect(onView).toHaveBeenCalled()
    })
  })
})

describe('CoverPanel under the Tracks filter', () => {
  it('says a cover goes to every file, including the ones the filter hides; nothing without a filter', async () => {
    const { root } = await mountPanel({ hiddenByFilter: 4 })
    expect(root.querySelector('.cover')?.textContent).toContain('including the 4 the filter hides')
    cleanup?.()
    const plain = await mountPanel()
    expect(plain.root.querySelector('.cover')?.textContent).not.toContain('filter hides')
  })
})
