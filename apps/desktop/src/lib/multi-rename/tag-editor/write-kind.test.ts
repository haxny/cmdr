/** The remembered write kind: Audiobook first, then whatever the user last picked; junk on disk reads as the default. */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { loadWriteKind, saveWriteKind } from './write-kind'

const disk = vi.hoisted(() => ({ kind: null as string | null }))
vi.mock('#lib/app-status-store.js', () => ({
  loadTagWriteKind: vi.fn(() => Promise.resolve(disk.kind)),
  saveTagWriteKind: vi.fn((kind: string) => {
    disk.kind = kind
    return Promise.resolve()
  }),
}))

describe('the remembered write kind', () => {
  beforeEach(() => {
    disk.kind = null
  })

  it('is Audiobook before the user ever picks one', async () => {
    expect(await loadWriteKind()).toBe('audiobook')
  })

  it('reads back the last pick', async () => {
    saveWriteKind('music')
    expect(await loadWriteKind()).toBe('music')
  })

  it('reads a stored value that isn’t a kind as Audiobook', async () => {
    disk.kind = 'podcast'
    expect(await loadWriteKind()).toBe('audiobook')
  })
})
