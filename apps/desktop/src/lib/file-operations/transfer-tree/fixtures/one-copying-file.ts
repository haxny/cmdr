/**
 * The smallest live view: one file a quarter copied. For tests of a surface that
 * embeds the tree (the progress dialog, a queue row), where the tree's own
 * behavior isn't under test, only that it's there and pulls.
 */

import type { TransferTreeView } from '#lib/tauri-commands/index.js'

export const oneCopyingFile: TransferTreeView = {
  ready: true,
  revision: 1,
  totalRows: 1,
  offset: 0,
  rows: [
    {
      id: 1,
      depth: 0,
      label: 'holiday.mov',
      kind: 'file',
      state: 'active',
      expanded: false,
      hasChildren: false,
      posInSet: 1,
      setSize: 1,
      filesDone: 0,
      filesTotal: 1,
      bytesDone: 1_000,
      bytesTotal: 4_000,
      skipped: 0,
      failed: 0,
      failure: null,
    },
  ],
  ancestors: [],
  focusRow: 0,
  axis: 'bytes',
}
