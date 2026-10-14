/**
 * Asking the backend for the files-changed counts of the commit rows on screen
 * that the walk left pending: only those rows, each once per listing.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FileEntry } from '../types'

const { fill } = vi.hoisted(() => ({ fill: vi.fn() }))
vi.mock('#lib/tauri-commands/index.js', () => ({ fillGitCommitCounts: fill }))

import { requestPendingCommitCounts, _resetCommitCountFillForTests } from './commit-count-fill'

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

describe('requestPendingCommitCounts', () => {
  beforeEach(() => {
    fill.mockReset()
    fill.mockResolvedValue(undefined)
    _resetCommitCountFillForTests()
  })

  it('asks for the pending rows only', () => {
    requestPendingCommitCounts('listing-1', [row('aaaaaaa', pending), row('bbbbbbb', counted), row('README.md')])
    expect(fill).toHaveBeenCalledWith('listing-1', ['/repo/.git/commits/aaaaaaa'])
  })

  it('asks for nothing when nothing on screen is pending', () => {
    requestPendingCommitCounts('listing-1', [row('bbbbbbb', counted)])
    expect(fill).not.toHaveBeenCalled()
  })

  it('asks for a row once per listing, however often the window is re-fetched', () => {
    requestPendingCommitCounts('listing-1', [row('aaaaaaa', pending)])
    requestPendingCommitCounts('listing-1', [row('aaaaaaa', pending), row('ccccccc', pending)])
    expect(fill).toHaveBeenNthCalledWith(2, 'listing-1', ['/repo/.git/commits/ccccccc'])

    requestPendingCommitCounts('listing-2', [row('aaaaaaa', pending)])
    expect(fill).toHaveBeenNthCalledWith(3, 'listing-2', ['/repo/.git/commits/aaaaaaa'])
  })
})
