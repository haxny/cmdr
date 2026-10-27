/** The cover panel: the shared cover, «multiple covers» or none; the folder picker; Remove; read-only locks both. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { CoverSummary } from '#lib/ipc/bindings.js'
import CoverPanel from './CoverPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { mount, tick, unmount } from 'svelte'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const SAME: CoverSummary = { kind: 'same', mime: 'image/jpeg', bytes: 2048, width: 600, height: 400 }

async function mountPanel(
  over: {
    cover?: CoverSummary
    editable?: boolean
    candidates?: { fileName: string; size: number }[] | null
    loadImage?: () => Promise<{ mime: string; data: number[] } | null>
  } = {},
) {
  const props = {
    cover: over.cover ?? SAME,
    editable: over.editable ?? true,
    listCandidates: vi.fn(() => Promise.resolve(over.candidates === undefined ? [] : over.candidates)),
    loadImage: over.loadImage ?? vi.fn(() => Promise.resolve(null)),
    onPick: vi.fn(),
    onEdit: vi.fn(),
  }
  const mounted = await mountInto(CoverPanel, props)
  cleanup = mounted.cleanup
  return { root: mounted.root, ...props }
}

function button(root: HTMLElement, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll('button')].find((b) => b.textContent.trim().startsWith(text))
  if (!found) throw new Error(`no ${text} button`)
  return found
}

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

describe('CoverPanel', () => {
  it('shows the shared cover: type, pixel size, file size', async () => {
    const { root } = await mountPanel()
    expect(root.querySelector('.summary')?.textContent).toBe('image/jpeg, 600×400 px, 2.00 KiB')
  })

  it('leaves the pixel size out when the header could not be read', async () => {
    const { root } = await mountPanel({ cover: { ...SAME, width: null, height: null } as CoverSummary })
    expect(root.querySelector('.summary')?.textContent).toBe('image/jpeg, 2.00 KiB')
  })

  it('says «multiple covers» and «no cover»', async () => {
    const multiple = await mountPanel({ cover: { kind: 'multiple' } })
    expect(multiple.root.querySelector('.summary')?.textContent).toBe('Multiple covers')
    multiple.root.remove()
    cleanup?.()
    const none = await mountPanel({ cover: { kind: 'empty' } })
    expect(none.root.querySelector('.summary')?.textContent).toBe('No cover')
    expect(button(none.root, 'Remove').disabled).toBe(true)
  })

  it('cover_from_folder_flow', async () => {
    const { root, listCandidates, onPick } = await mountPanel({
      candidates: [
        { fileName: 'cover.jpg', size: 1024 },
        { fileName: 'back.png', size: 4096 },
      ],
    })
    button(root, 'From folder').click()
    await settle()
    expect(listCandidates).toHaveBeenCalledTimes(1)
    const items = [...root.querySelectorAll<HTMLButtonElement>('.candidate')]
    expect(items.map((i) => i.textContent.trim())).toEqual(['cover.jpg (1.00 KiB)', 'back.png (4.00 KiB)'])
    items[1].click()
    await settle()
    expect(onPick).toHaveBeenCalledWith('back.png')
    // The list closes after a pick.
    expect(root.querySelector('.candidates')).toBeNull()
  })

  it('says so when the folder has no pictures, and when listing failed', async () => {
    const empty = await mountPanel({ candidates: [] })
    button(empty.root, 'From folder').click()
    await settle()
    expect(empty.root.textContent).toContain('No pictures (jpg, png, webp) in this folder.')
    cleanup?.()
    const failed = await mountPanel({ candidates: null })
    button(failed.root, 'From folder').click()
    await settle()
    expect(failed.root.querySelector('[role="alert"]')?.textContent).toContain('Couldn’t list the pictures')
  })

  it('remove_cover', async () => {
    const { root, onEdit } = await mountPanel()
    button(root, 'Remove').click()
    expect(onEdit).toHaveBeenCalledWith([{ kind: 'cover', edit: 'remove' }])
  })

  it('read_only_disables_inputs', async () => {
    const { root, listCandidates } = await mountPanel({ editable: false })
    expect(button(root, 'From folder').disabled).toBe(true)
    expect(button(root, 'Remove').disabled).toBe(true)
    button(root, 'From folder').click()
    expect(listCandidates).not.toHaveBeenCalled()
  })

  describe('thumbnail', () => {
    const urls = { created: [] as string[], revoked: [] as string[] }
    beforeEach(() => {
      urls.created = []
      urls.revoked = []
      vi.stubGlobal('URL', {
        ...URL,
        createObjectURL: vi.fn(() => {
          const url = `blob:cover-${String(urls.created.length)}`
          urls.created.push(url)
          return url
        }),
        revokeObjectURL: vi.fn((url: string) => urls.revoked.push(url)),
      })
    })
    afterEach(() => {
      vi.unstubAllGlobals()
    })

    it('shows the shared cover as a blob URL and revokes it on unmount', async () => {
      const loadImage = vi.fn(() => Promise.resolve({ mime: 'image/jpeg', data: [1, 2, 3] }))
      const { root } = await mountPanel({ loadImage })
      await settle()
      expect(loadImage).toHaveBeenCalledTimes(1)
      expect(root.querySelector('img.thumbnail')?.getAttribute('src')).toBe('blob:cover-0')
      cleanup?.()
      cleanup = undefined
      expect(urls.revoked).toEqual(['blob:cover-0'])
    })

    it('has no thumbnail for no cover or multiple covers, and asks for none', async () => {
      const loadImage = vi.fn(() => Promise.resolve(null))
      const { root } = await mountPanel({ cover: { kind: 'multiple' }, loadImage })
      await settle()
      expect(loadImage).not.toHaveBeenCalled()
      expect(root.querySelector('img')).toBeNull()
    })

    it('a changed cover replaces the thumbnail and revokes the old URL; a late old answer is dropped', async () => {
      const answers: ((image: { mime: string; data: number[] }) => void)[] = []
      const loadImage = vi.fn(
        () => new Promise<{ mime: string; data: number[] } | null>((resolve) => answers.push(resolve)),
      )
      const props = $state({
        cover: SAME,
        editable: true,
        listCandidates: () => Promise.resolve([]),
        loadImage,
        onPick: () => {},
        onEdit: () => {},
      })
      const root = document.createElement('div')
      document.body.appendChild(root)
      const panel = mount(CoverPanel, { target: root, props })
      cleanup = () => {
        void unmount(panel)
        root.remove()
      }
      await settle()
      props.cover = { ...SAME, bytes: 4096 }
      await settle()
      expect(answers).toHaveLength(2)
      // The newer answer lands first, then the stale one.
      answers[1]({ mime: 'image/jpeg', data: [2] })
      await settle()
      answers[0]({ mime: 'image/jpeg', data: [1] })
      await settle()
      expect(urls.created).toEqual(['blob:cover-0'])
      expect(root.querySelector('img.thumbnail')?.getAttribute('src')).toBe('blob:cover-0')
      // Removing the cover drops the thumbnail and revokes its URL.
      props.cover = { kind: 'empty' }
      await settle()
      expect(root.querySelector('img')).toBeNull()
      expect(urls.revoked).toEqual(['blob:cover-0'])
    })
  })

  it('a second press while the first listing is out closes the picker for good', async () => {
    let answer: (list: { fileName: string; size: number }[]) => void = () => {}
    const props = {
      cover: SAME,
      editable: true,
      listCandidates: vi.fn(
        () =>
          new Promise<{ fileName: string; size: number }[]>((resolve) => {
            answer = resolve
          }),
      ),
      loadImage: () => Promise.resolve(null),
      onPick: () => {},
      onEdit: () => {},
    }
    const second = await mountInto(CoverPanel, props)
    cleanup = second.cleanup
    button(second.root, 'From folder').click()
    await settle()
    button(second.root, 'From folder').click()
    await settle()
    answer([{ fileName: 'late.jpg', size: 1 }])
    await settle()
    expect(second.root.querySelector('.candidates')).toBeNull()
  })
})
