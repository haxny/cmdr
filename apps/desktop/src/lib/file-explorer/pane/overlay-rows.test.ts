import { describe, it, expect, vi, beforeEach } from 'vitest'

const ipc = vi.hoisted(() => ({
  findFileIndices: vi.fn(),
  getFileAt: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getShowVirtualGitPortal: () => true }))

import { findOverlayRowIndices, isOverlayRow, overlayRowCandidates, overlayRowsToSkip } from './overlay-rows'

const row = (name: string, iconId: string) => ({ name, iconId })

describe('isOverlayRow', () => {
  it("is a restic repo's `Browse snapshots` row in the repo's own folder", () => {
    expect(isOverlayRow(row('Browse snapshots', 'restic:snapshots'), 'local')).toBe(true)
    expect(isOverlayRow(row('Browse snapshots', 'restic:snapshots'), 'smb')).toBe(true)
  })

  it('is a git portal category row in a real `.git` folder', () => {
    expect(isOverlayRow(row('branches', 'git:branch'), 'local')).toBe(true)
  })

  it('is no real folder, whatever its name', () => {
    expect(isOverlayRow(row('Browse snapshots', 'dir'), 'local')).toBe(false)
    expect(isOverlayRow(row('branches', 'dir'), 'local')).toBe(false)
  })

  it('is no row of a pane already inside a view: those rows ARE the content', () => {
    expect(isOverlayRow(row('main', 'git:branch'), 'git-portal')).toBe(false)
    expect(isOverlayRow(row('latest', 'dir'), 'restic')).toBe(false)
  })
})

describe('overlayRowCandidates', () => {
  it('names the restic row anywhere, and the git categories only in a `.git` folder with the portal on', () => {
    expect(overlayRowCandidates('/r', true)).toEqual(['Browse snapshots'])
    expect(overlayRowCandidates('/repo/.git', true)).toEqual([
      'Browse snapshots',
      'branches',
      'tags',
      'commits',
      'stash',
      'worktrees',
      'submodules',
    ])
    expect(overlayRowCandidates('/repo/.git', false)).toEqual(['Browse snapshots'])
  })
})

describe('findOverlayRowIndices', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  const base = { listingId: 'l1', includeHidden: false, panePath: '/r', paneKind: 'local' as const, gitPortalOn: true }

  it('finds the view row and returns its pane index, past `..`', async () => {
    ipc.findFileIndices.mockResolvedValue({ 'Browse snapshots': 0 })
    ipc.getFileAt.mockResolvedValue(row('Browse snapshots', 'restic:snapshots'))

    const found = await findOverlayRowIndices({ ...base, hasParent: true })

    expect([...found]).toEqual([1])
    expect(ipc.getFileAt).toHaveBeenCalledWith('l1', 0, false)
  })

  it('keeps a real folder that merely shares the name', async () => {
    ipc.findFileIndices.mockResolvedValue({ 'Browse snapshots': 3 })
    ipc.getFileAt.mockResolvedValue(row('Browse snapshots', 'dir'))

    expect((await findOverlayRowIndices({ ...base, hasParent: false })).size).toBe(0)
  })

  it('asks nothing in a pane inside a view', async () => {
    const found = await findOverlayRowIndices({ ...base, paneKind: 'restic', hasParent: true })
    expect(found.size).toBe(0)
    expect(ipc.findFileIndices).not.toHaveBeenCalled()
  })

  it('skips nothing when the lookup throws, rather than failing the select-all', async () => {
    ipc.findFileIndices.mockRejectedValue(new Error('listing gone'))
    const { listingId, includeHidden, panePath, paneKind } = base
    expect((await overlayRowsToSkip({ listingId, includeHidden, panePath, paneKind, hasParent: true })).size).toBe(0)
  })

  it('costs one lookup and nothing more in an ordinary folder', async () => {
    ipc.findFileIndices.mockResolvedValue({})
    expect((await findOverlayRowIndices({ ...base, hasParent: true })).size).toBe(0)
    expect(ipc.getFileAt).not.toHaveBeenCalled()
  })
})
