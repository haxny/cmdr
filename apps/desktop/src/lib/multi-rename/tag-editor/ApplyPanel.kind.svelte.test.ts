/**
 * The write kind: remembered for this user (Audiobook on first run), sent with the change list and the write, and the
 * list names what the kind removes beyond the fields (an MP3's conductor under Audiobook).
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import ApplyPanel from './ApplyPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { CHANGES, asState, fakeState, press, settle } from './apply-panel.test-helper'

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
    expect(fake.apply).toHaveBeenCalledWith(
      { kind: 'music', allowFrameLoss: false, skip: [], allowRemovals: [] },
      { after: [] },
    )
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
      value: [{ ...CHANGES[1], fields: ['album'], extras: false, removes: ['conductor'], removesUnknown: false }],
    })
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(listed(root)).toEqual(['02.mp3: Album, conductor (removed)'])
  })

  /** N1: the list says what the write does, so the write uses the kind the list was made with. */
  it('a remembered kind that arrives after the list was made changes neither the list nor the write', async () => {
    let remember: (kind: string | null) => void = () => {}
    store.loadTagWriteKind.mockImplementationOnce(() => new Promise((resolve) => (remember = resolve)))
    const { root, fake } = await mountPanel()
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenCalledWith('audiobook')
    remember('music')
    await press(root, 'Write tags')
    expect(root.textContent).not.toContain('Writing as Music')
    expect(fake.apply).toHaveBeenCalledWith(
      { kind: 'audiobook', allowFrameLoss: false, skip: [], allowRemovals: [] },
      { after: [] },
    )
  })

  it('listed as Music, a switch to Audiobook before the list shows can’t make the write drop the conductor unlisted', async () => {
    disk.kind = 'music'
    const fake = fakeState()
    let answer: (res: unknown) => void = () => {}
    fake.changes.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)) as never)
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenCalledWith('music')
    // While the list is on its way the choice is locked; a click doesn't move it.
    await press(root, 'Audiobook')
    expect(store.saveTagWriteKind).not.toHaveBeenCalled()
    answer({ ok: true, value: CHANGES })
    await settle()
    expect(root.textContent).toContain('Writing as Music')
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenCalledWith(
      { kind: 'music', allowFrameLoss: false, skip: [], allowRemovals: [] },
      { after: [] },
    )
  })

  /** N4: a wait that gave up (a call still out) lists nothing and writes nothing. */
  it('a timed-out wait before the list lists nothing; one before the write writes nothing', async () => {
    const fake = fakeState()
    fake.settled.mockResolvedValueOnce(false)
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(fake.changes).not.toHaveBeenCalled()
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenCalledTimes(1)
    fake.settled.mockResolvedValueOnce(false)
    await press(root, 'Write tags')
    expect(fake.apply).not.toHaveBeenCalled()
    expect(listed(root)).toHaveLength(2)
  })
})
