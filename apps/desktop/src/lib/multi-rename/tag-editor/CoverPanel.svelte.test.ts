/** The cover panel: the shared cover, «multiple covers» or none; the folder picker; Remove; read-only locks both. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { CoverSummary } from '#lib/ipc/bindings.js'
import CoverPanel from './CoverPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { tick } from 'svelte'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const SAME: CoverSummary = { kind: 'same', mime: 'image/jpeg', bytes: 2048, width: 600, height: 400 }

async function mountPanel(
  over: { cover?: CoverSummary; editable?: boolean; candidates?: { fileName: string; size: number }[] | null } = {},
) {
  const props = {
    cover: over.cover ?? SAME,
    editable: over.editable ?? true,
    listCandidates: vi.fn(() => Promise.resolve(over.candidates === undefined ? [] : over.candidates)),
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
})
