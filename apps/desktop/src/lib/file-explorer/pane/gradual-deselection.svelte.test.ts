/**
 * Gradual deselection: each source of the operation this pane started leaves the
 * selection as it finishes (`write-source-item-done`), and only that operation's.
 * The queue runs operations on other lanes at once, and one of them finishing a
 * same-named file must not deselect this pane's row.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import type { WriteSourceItemDoneEvent } from '#lib/tauri-commands/index.js'
import {
  beginForegroundClaim,
  endForegroundClaim,
  setForegroundOperationId,
} from '#lib/file-operations/foreground-operation.svelte.js'
import { initListingDiffSync, type ListingDiffSyncDeps } from './listing-diff-sync.svelte'
import { createPaneRowState } from './pane-row-state'

const ipc = vi.hoisted(() => ({
  sourceDone: null as ((event: WriteSourceItemDoneEvent) => void) | null,
  findFileIndex: vi.fn((_listingId: string, name: string) => Promise.resolve(name === 'a.txt' ? 3 : null)),
  getSelectionSnapshot: vi.fn(() => Promise.resolve({ paths: [], fileCount: 0, folderCount: 0 })),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({
  onDirectoryDiff: () => Promise.resolve(() => {}),
  onDirectoryDeleted: () => Promise.resolve(() => {}),
  onListingRespelled: () => Promise.resolve(() => {}),
  onWriteSourceItemDone: (callback: (event: WriteSourceItemDoneEvent) => void) => {
    ipc.sourceDone = callback
    return Promise.resolve(() => {})
  },
  findFileIndex: ipc.findFileIndex,
  getSelectionSnapshot: ipc.getSelectionSnapshot,
}))
vi.mock('#lib/logging/logger.js', () => ({ getAppLogger: () => ({ info: vi.fn() }) }))

let dispose = () => {}
afterEach(() => {
  dispose()
  setForegroundOperationId(null)
  vi.clearAllMocks()
})

/**
 * A pane whose `a.txt` (row 3) is selected for the operation it started. `'all'` is the
 * ⌘A shape: rows 1–4 selected, and the snapshot holds `'all'` in place of a name list.
 */
function pane(snapshot: 'names' | 'all' = 'names') {
  const selected = new SvelteSet(snapshot === 'all' ? [1, 2, 3, 4] : [3])
  const rows = createPaneRowState({ getListingId: () => 'L', getLoading: () => false, getOperationActive: () => true })
  rows.initialize()
  const names = snapshot === 'all' ? 'all' : ['a.txt']
  const deps = {
    rowState: rows,
    selection: { selectedIndices: selected, getSelectedIndices: () => [...selected] },
    rename: { active: false },
    renameFlow: { pendingCursorName: null },
    getListingId: () => 'L',
    getIncludeHidden: () => true,
    getHasParent: () => false,
    getOperationSelectedNames: () => names,
  } as unknown as ListingDiffSyncDeps
  dispose = $effect.root(() => {
    initListingDiffSync(deps)
  })
  flushSync()
  return selected
}

function done(operationId: string): void {
  if (!ipc.sourceDone) throw new Error('the listener never registered')
  ipc.sourceDone({ operationId, sourcePath: '/src/a.txt', outcome: 'done', sourceRemoved: false })
}

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await new Promise((resolve) => setTimeout(resolve, 0))
  flushSync()
}

describe('gradual deselection', () => {
  it('deselects a source of the operation this pane started as it finishes', async () => {
    setForegroundOperationId('op-mine')
    const selected = pane()
    done('op-mine')
    await settle()
    expect(selected.has(3)).toBe(false)
  })

  it('deselects each finished source of a ⌘A operation and leaves the rest selected', async () => {
    setForegroundOperationId('op-mine')
    const selected = pane('all')
    done('op-mine')
    await settle()
    expect([...selected].sort()).toEqual([1, 2, 4])
  })

  it('leaves a same-named row alone when another operation finishes its own', async () => {
    setForegroundOperationId('op-mine')
    const selected = pane()
    done('op-other-lane')
    await settle()
    expect(selected.has(3)).toBe(true)
    expect(ipc.findFileIndex).not.toHaveBeenCalled()
  })

  it('holds an event that beats the start command’s answer until the operation is named', async () => {
    beginForegroundClaim()
    const selected = pane()
    done('op-other-lane')
    done('op-mine')
    await settle()
    expect(selected.has(3)).toBe(true)

    setForegroundOperationId('op-mine')
    endForegroundClaim()
    await settle()
    expect(selected.has(3)).toBe(false)
    expect(ipc.findFileIndex).toHaveBeenCalledTimes(1)
  })
})
