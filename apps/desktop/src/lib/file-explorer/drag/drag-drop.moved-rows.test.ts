/**
 * A selection drag whose rows moved under it: a watcher insert lands above the
 * selection between the pane applying its rows and the drag threshold, so the
 * old row numbers name the neighbor (`pane/moved-listing.test-fixtures.ts`).
 * Dragging that one to another pane or the Trash moves a file nobody picked.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

const { dragged, addToastSpy } = vi.hoisted(() => ({
  dragged: [] as string[],
  addToastSpy: vi.fn(),
}))

vi.mock('@tauri-apps/api/path', () => ({
  tempDir: () => Promise.resolve('/tmp'),
  join: (...parts: string[]) => Promise.resolve(parts.join('/')),
}))
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: () => Promise.resolve() }))
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn() }))
vi.mock('#lib/icon-cache.js', () => ({
  getCachedIcon: () => 'data:image/png;base64,AA==',
  getCachedCustomFolderIcon: () => undefined,
}))
vi.mock('#lib/tauri-commands/index.js', async () => {
  const moved = await import('../pane/moved-listing.test-fixtures')
  const { asListingLookupError } = await vi.importActual<typeof import('#lib/tauri-commands/listing-gone.js')>(
    '#lib/tauri-commands/listing-gone.js',
  )
  return {
    asListingLookupError,
    getSelectionSnapshot: moved.selectionSnapshot,
    startDragPaths: (paths: string[]) => {
      dragged.push(...paths)
      return Promise.resolve()
    },
    prepareSelfDragOverlay: () => Promise.resolve(),
    clearSelfDragOverlay: () => Promise.resolve(),
    setSelfDragResolvedOperation: () => Promise.resolve(),
  }
})
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: addToastSpy }))
vi.mock('#lib/settings/settings-store.js', () => ({ getSetting: vi.fn(() => 5) }))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('./drag-image-renderer', () => ({ renderDragImage: () => Promise.reject(new Error('no canvas here')) }))

import { startSelectionDragTracking, getSelfDragIdentity, cancelDragTracking } from './drag-drop'
import { APPLIED_SEQUENCE, MOVED_LISTING_ID, SELECTED_PATH, SELECTED_ROW } from '../pane/moved-listing.test-fixtures'

/** Presses on a row and moves past the drag threshold, then lets the drag start settle. */
async function dragSelection(expectedSequence: number) {
  startSelectionDragTracking(new MouseEvent('mousedown', { clientX: 0, clientY: 0 }), {
    type: 'selection',
    listingId: MOVED_LISTING_ID,
    indices: [SELECTED_ROW],
    includeHidden: false,
    hasParent: false,
    expectedSequence,
    sourceVolumeId: 'root',
    iconId: 'ext:txt',
  })
  document.dispatchEvent(new MouseEvent('mousemove', { clientX: 50, clientY: 0 }))
  await vi.waitFor(() => {
    expect(dragged.length > 0 || addToastSpy.mock.calls.length > 0).toBe(true)
  })
}

describe('a selection drag when the rows moved under the press', () => {
  beforeEach(() => {
    dragged.length = 0
    addToastSpy.mockClear()
    cancelDragTracking()
  })

  it('refuses rather than dragging the file that slid onto the selected row', async () => {
    await dragSelection(APPLIED_SEQUENCE)

    expect(dragged).toEqual([])
    expect(getSelfDragIdentity()).toBeNull()
    expect(addToastSpy).toHaveBeenCalledWith(
      'The list here is still updating, so nothing started. Try again in a moment.',
      { level: 'info' },
    )
  })

  it('drags the selected file and records it as the identity while the rows still match', async () => {
    const { BACKEND_SEQUENCE } = await import('../pane/moved-listing.test-fixtures')
    // Against the backend's current revision, row 2 is the same `charlie.txt`.
    startSelectionDragTracking(new MouseEvent('mousedown', { clientX: 0, clientY: 0 }), {
      type: 'selection',
      listingId: MOVED_LISTING_ID,
      indices: [2],
      includeHidden: false,
      hasParent: false,
      expectedSequence: BACKEND_SEQUENCE,
      sourceVolumeId: 'root',
      iconId: 'ext:txt',
    })
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 50, clientY: 0 }))
    await vi.waitFor(() => {
      expect(dragged).toEqual([SELECTED_PATH])
    })
    expect(getSelfDragIdentity()?.sourcePaths).toEqual([SELECTED_PATH])
  })
})
