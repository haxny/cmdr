/**
 * What the rename review says about itself, derived from its state: how far the check has got,
 * how many rows it never finished, and the words for a typed refusal. Pure, so the dialog stays
 * markup and the wording stays testable.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { getUserFriendlyMessage } from '#lib/file-operations/transfer/transfer-error-messages.js'
import type { BulkRenameReviewProposal, RenameReviewNotice } from './ask-cmdr-state.svelte'

/** A batch still answerable: an expired or changed one has nothing left to check. */
function isLive(proposal: BulkRenameReviewProposal): boolean {
  return !proposal.expired && !proposal.changed
}

/**
 * A live batch whose check didn't finish. Its rows can't be renamed until one does, and only
 * the user restarts it: rechecking by itself is how a toggle on a slow volume used to start
 * the same doomed check over and over.
 */
export function isUnchecked(proposal: BulkRenameReviewProposal): boolean {
  return isLive(proposal) && proposal.checkFailed
}

/** How many rows are waiting on Check again. */
export function uncheckedRowCount(proposals: BulkRenameReviewProposal[]): number {
  return proposals.filter(isUnchecked).reduce((count, proposal) => count + proposal.rows.length, 0)
}

/**
 * The whole job's progress, or `null` while no check has taken long enough to report one.
 *
 * Every batch that reported stays in the sum until the last one ends (the slice marks a finished
 * one complete), so the count only ever climbs while a job's batches finish at different times.
 */
export function renameCheckProgress(proposals: BulkRenameReviewProposal[]): { checked: number; total: number } | null {
  const reporting = proposals.flatMap((proposal) => (proposal.checkProgress ? [proposal.checkProgress] : []))
  if (reporting.length === 0) return null
  return {
    checked: reporting.reduce((sum, progress) => sum + progress.checked, 0),
    total: reporting.reduce((sum, progress) => sum + progress.total, 0),
  }
}

/** The words for a review-level notice. `detail` is HTML from the error pipeline, which escapes
 *  what it interpolates, so it renders the way `FallbackErrorContent` renders it. */
export function renameNoticeWords(notice: RenameReviewNotice): { message: string; detail: string | null } {
  switch (notice.kind) {
    case 'needsAnotherLook':
      return { message: tString('askCmdr.renameReview.needsAnotherLook'), detail: null }
    case 'nothingToApply':
      return { message: tString('askCmdr.renameReview.nothingToApply'), detail: null }
    case 'nameNotSaved':
      return { message: tString('askCmdr.renameReview.nameNotSaved'), detail: null }
    case 'couldntStart':
      return {
        message: tString('askCmdr.renameReview.couldntStart'),
        // A batch that wouldn't start ran nothing, and a rename is a move in the error catalog's
        // terms: the file stays in place under a new name.
        detail: notice.reason?.type === 'engine' ? getUserFriendlyMessage(notice.reason.error, 'move').message : null,
      }
  }
}
