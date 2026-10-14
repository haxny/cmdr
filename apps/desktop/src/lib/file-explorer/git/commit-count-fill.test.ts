/**
 * Getting the files-changed counts the commit walk leaves out: one whole-listing
 * pass when the backend takes it, otherwise the rows on screen, each once per
 * listing. And the answer the Size-sort refusal reads.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FileEntry } from '../types'

const { ipc } = vi.hoisted(() => ({ ipc: { fill: vi.fn(), countAll: vi.fn() } }))
vi.mock('#lib/tauri-commands/index.js', () => ({
  fillGitCommitCounts: ipc.fill,
  countAllGitCommits: ipc.countAll,
}))

import { requestPendingCommitCounts, commitSizeSortLimit, _resetCommitCountFillForTests } from './commit-count-fill'

function row(name: string, gitMeta?: FileEntry['gitMeta']): FileEntry {
  return {
    name,
    path: `/repo/.git/commits/${name}`,
    isDirectory: true,
    isSymlink: false,
    permissions: 0o755,
    owner: '',
    group: '',
    iconId: 'git:commit',
    extendedMetadataLoaded: true,
    gitMeta,
  }
}

const pending = { kind: 'countPending', counted: 'filesChanged' } as const
const counted = { kind: 'count', counted: 'filesChanged', n: 3 } as const

/** Lets the whole-listing answer land. */
const settle = () => new Promise((r) => setTimeout(r, 0))

describe('requestPendingCommitCounts', () => {
  beforeEach(() => {
    ipc.fill.mockReset().mockResolvedValue(undefined)
    ipc.countAll.mockReset()
    _resetCommitCountFillForTests()
  })

  it('asks for the whole-listing pass once, and no row-by-row fill while it runs', async () => {
    ipc.countAll.mockResolvedValue({ type: 'counting' })
    requestPendingCommitCounts('listing-1', [row('aaaaaaa', pending)])
    await settle()
    requestPendingCommitCounts('listing-1', [row('bbbbbbb', pending)])
    await settle()

    expect(ipc.countAll).toHaveBeenCalledTimes(1)
    expect(ipc.countAll).toHaveBeenCalledWith('listing-1')
    expect(ipc.fill).not.toHaveBeenCalled()
    expect(commitSizeSortLimit('listing-1')).toBeNull()
  })

  it('falls back to the rows on screen when the listing is too long, each row once', async () => {
    ipc.countAll.mockResolvedValue({ type: 'tooMany', limit: 5000 })
    requestPendingCommitCounts('listing-1', [row('aaaaaaa', pending), row('bbbbbbb', counted)])
    await settle()
    expect(ipc.fill).toHaveBeenCalledWith('listing-1', ['/repo/.git/commits/aaaaaaa'])

    requestPendingCommitCounts('listing-1', [row('aaaaaaa', pending), row('ccccccc', pending)])
    await settle()
    expect(ipc.fill).toHaveBeenLastCalledWith('listing-1', ['/repo/.git/commits/ccccccc'])
    expect(ipc.countAll).toHaveBeenCalledTimes(1)
  })

  it('remembers the limit for the Size-sort refusal', async () => {
    ipc.countAll.mockResolvedValue({ type: 'tooMany', limit: 5000 })
    requestPendingCommitCounts('listing-1', [row('aaaaaaa', pending)])
    await settle()

    expect(commitSizeSortLimit('listing-1')).toBe(5000)
    expect(commitSizeSortLimit('listing-2')).toBeNull()
  })

  it('asks for nothing when nothing on screen is pending', async () => {
    requestPendingCommitCounts('listing-1', [row('bbbbbbb', counted)])
    await settle()
    expect(ipc.countAll).not.toHaveBeenCalled()
    expect(ipc.fill).not.toHaveBeenCalled()
  })
})
