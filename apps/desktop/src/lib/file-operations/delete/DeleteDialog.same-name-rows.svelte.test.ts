/**
 * `DeleteDialog.svelte`: a selection whose items share a NAME across folders.
 *
 * Only a snapshot pane (search results) can hand the dialog that: a real folder can't
 * hold two `.DS_Store` files, but a search for `.DS_Store` under a tree returns one per
 * folder. Regression anchor for ERR-VEZUB (0.54.0): select all in such a result set,
 * press F8, and the dialog threw while rendering, every time.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'

vi.mock('#lib/tauri-commands/index.js', () => ({
  estimateOperationCost: vi.fn(() => Promise.resolve([])),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatFileSize: vi.fn((n: number | undefined) => (n === undefined ? '' : `${String(n)} B`)),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

const ROOT = '/Users/test/Pictures/Trips'

/** What `openDeleteFromSearchResults` builds from a snapshot: one item per hit, `sourcePaths` index-aligned. */
function mountDialog(paths: string[]): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DeleteDialog, {
    target,
    props: {
      sourceItems: paths.map((path) => ({
        name: path.slice(path.lastIndexOf('/') + 1),
        isDirectory: false,
        isSymlink: false,
        size: 6148,
      })),
      sourcePaths: paths,
      sourceFolderPath: ROOT,
      isPermanent: false,
      supportsTrash: true,
      isFromCursor: false,
      sourceVolumeId: 'root',
      onConfirm: () => {},
      onCancel: () => {},
    },
  })
  return target
}

describe('DeleteDialog with same-named items from different folders', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders 19 `.DS_Store` hits from 19 folders (the ERR-VEZUB shape)', async () => {
    const paths = Array.from({ length: 19 }, (_, i) => `${ROOT}/Target ${String(i)}/.DS_Store`)
    const target = mountDialog(paths)
    await tick()

    const rows = target.querySelectorAll('.item-name')
    expect(rows).toHaveLength(10)
    expect([...rows].every((row) => row.textContent === '.DS_Store')).toBe(true)
  })

  it('renders two non-hidden files that share a name', async () => {
    const target = mountDialog([`${ROOT}/a/notes.txt`, `${ROOT}/b/notes.txt`])
    await tick()

    expect(target.querySelectorAll('.item-name')).toHaveLength(2)
  })

  it('renders distinct names (control: the dialog itself is fine)', async () => {
    const target = mountDialog([`${ROOT}/a/one.txt`, `${ROOT}/b/two.txt`])
    await tick()

    expect(target.querySelectorAll('.item-name')).toHaveLength(2)
  })

  it('renders when the duplicate sits past the 10 visible rows', async () => {
    const unique = Array.from({ length: 10 }, (_, i) => `${ROOT}/f${String(i)}.txt`)
    const target = mountDialog([...unique, `${ROOT}/x/f0.txt`])
    await tick()

    expect(target.querySelectorAll('.item-name')).toHaveLength(10)
  })
})
