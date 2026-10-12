/**
 * The `transfer-progress` preview: a REAL-OPERATION row the harness renders.
 *
 * `TransferProgressDialog` starts its own operation on mount and shows only
 * what that operation reports: the scan, the bars, the live file tree, pause
 * and background. So the preview passes it real start props, a copy out of the
 * transfer fixture tree into its emptied `To/`, and the shipping dialog does
 * the rest. The backend paces copies out of that tree per file
 * (`test_mode::pace_copies_from`), so the tree is there to review; nothing else
 * the app copies is slowed.
 *
 * The copy really happens, inside the throwaway tree. Escape sends it to the
 * background like the real dialog's; the next trigger empties `To/` again.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import { openGalleryDialog } from './gallery-state.svelte'
import type { FixtureDirPayload } from './disk-fixture'
import { transferProgressFixtures } from './fixtures/transfer-progress'

const log = getAppLogger('dialogGallery')

/** What a trigger did, so the caller (and the test) can see which branch ran. */
export type TransferProgressPreviewOutcome =
  /** The harness is rendering the dialog, which starts the copy. */
  | { kind: 'opened' }
  /** No fixture tree came with the trigger, so nothing was opened. */
  | { kind: 'no-fixtures' }
  /** A state id the row doesn't advertise. Same "open nothing" rule as a missing fixture. */
  | { kind: 'unknown-state' }

/** Opens the progress dialog on a real copy of the shape `stateId` names. */
export function openTransferProgressPreview(
  stateId: string,
  fixtures: FixtureDirPayload | null,
): TransferProgressPreviewOutcome {
  const pick = transferProgressFixtures[stateId]
  if (pick === undefined) return { kind: 'unknown-state' }
  if (fixtures === null) {
    log.debug('Dialog gallery: no transfer fixture tree came with the trigger, so no preview')
    return { kind: 'no-fixtures' }
  }
  openGalleryDialog('transfer-progress', stateId, { transfer: pick(fixtures.transferPreview) })
  return { kind: 'opened' }
}
