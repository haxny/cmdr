/**
 * Where a transfer tree's rows come from: one window per call, under the
 * view's own hand toggles and scroll position. The backend owns the tree; a
 * source only answers `view`.
 *
 * Two implementations: {@link ipcTransferTreeSource} for a real operation, and
 * the scripted ones in `fixtures/` that replay a simulated transfer for the
 * Debug catalog and the tests.
 */

import { getTransferTreeView, type TransferTreeAnswer, type TransferTreeViewRequest } from '$lib/tauri-commands'

export interface TransferTreeSource {
  /** One window of rows, or the typed reason there's none. Never throws for a refusal. */
  view: (request: TransferTreeViewRequest) => Promise<TransferTreeAnswer>
}

/** The source for a running operation: `get_transfer_tree_view` on its id. */
export function ipcTransferTreeSource(operationId: string): TransferTreeSource {
  return {
    view: (request) => getTransferTreeView(operationId, request),
  }
}
