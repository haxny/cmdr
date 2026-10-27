/**
 * The write kind: remembered for this user (Audiobook on first run), sent with the change list and the write, and the
 * list names what the kind removes beyond the fields (an MP3's conductor under Audiobook).
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import ApplyPanel from './ApplyPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { CHANGES, asState, fakeState, press } from './apply-panel.test-helper'

const disk = vi.hoisted(() => ({ kind: null as string | null }))
const store = vi.hoisted(() => ({
  loadTagWriteKind: vi.fn(() => Promise.resolve(disk.kind)),
  saveTagWriteKind: vi.fn((kind: string) => {
    disk.kind = kind
    return Promise.resolve()
  }),
}))
vi.mock('#lib/app-status-store.js', () => store)

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  disk.kind = null
  vi.clearAllMocks()
})

async function mountPanel(fake = fakeState()) {
  const mounted = await mountInto(ApplyPanel, { tags: asState(fake) })
  cleanup = mounted.cleanup
  return { root: mounted.root, fake }
}

const listed = (root: HTMLElement) =>
  [...(root.querySelector('[aria-label="Files that will change"]')?.querySelectorAll('li') ?? [])].map((li) =>
    li.textContent.trim(),
  )

describe('ApplyPanel write kind', () => {
  it('opens on the kind last picked, and lists and writes as it', async () => {
    disk.kind = 'music'
    const { root, fake } = await mountPanel()
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenCalledWith('music')
    expect(root.textContent).toContain('Writing as Music: 2 files will change:')
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenCalledWith({ kind: 'music', allowFrameLoss: false, skip: [] }, { after: [] })
  })

  it('is Audiobook on first run, and a pick is remembered', async () => {
    const { root, fake } = await mountPanel()
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenLastCalledWith('audiobook')
    await press(root, 'Back')
    await press(root, 'Music')
    expect(store.saveTagWriteKind).toHaveBeenCalledWith('music')
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenLastCalledWith('music')
  })

  it('names the conductor an Audiobook write removes from an MP3', async () => {
    const fake = fakeState()
    fake.changes.mockResolvedValue({
      ok: true,
      value: [{ ...CHANGES[1], fields: ['album'], extras: false, removes: ['conductor'] }],
    })
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(listed(root)).toEqual(['02.mp3: Album, conductor (removed)'])
  })
})
