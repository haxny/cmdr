/**
 * The bulk-rename review: the state slice behind `BulkRenameReviewDialog.svelte`.
 *
 * A turn's `proposalReady` events stage plans here, the end of the turn opens ONE review over
 * all of them, every user decision and every relevant pane change revalidates it against the
 * backend, Apply starts one managed operation per staged plan, and each leaves an undo line in
 * the thread.
 *
 * The SERVER owns the outcome throughout: a name the user types goes over IPC and comes back
 * validated (never patched locally), preflight is what makes an edited name applicable at
 * all, and Apply sends opaque row ids, not paths.
 *
 * **Grouping is presentational, and every guardrail stays per row**: the evidence check, the
 * pane-scoped source validation, the fingerprinted preflight, and the fingerprint recheck at
 * write time all still run per row, inside the proposal that row belongs to.
 *
 * **A check has no time limit.** It reports progress and the user can stop it; a check that didn't
 * finish marks its batch (`checkFailed`), and only Check again restarts it.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import { LogOnceGate } from '#lib/logging/log-once.js'
import { failureOf } from '#lib/ipc/typed-failure.js'
import { SvelteSet } from 'svelte/reactivity'
import type { RailMessage } from './ask-cmdr-messages'
import {
  askCmdrState,
  type BulkRenameReview,
  type BulkRenameReviewProposal,
  type BulkRenameReviewRow,
} from './ask-cmdr-state.svelte'
import { isUnchecked } from './rename-review-status'
import { undoStateFromReport } from './rename-undo'
import { isOperationLogRecording } from '#lib/operation-log/recording.js'
import {
  applyBulkRename,
  BulkRenameFailure,
  cancelBulkRenameCheck,
  cancelBulkRenameProposal,
  preflightBulkRename,
  reviseBulkRenameRow,
  undoOperations,
  type AskCmdrStreamEvent,
  type BulkRenameCheckProgress,
  type BulkRenameError,
} from '#lib/tauri-commands/index.js'

const log = getAppLogger('askCmdr')

/** A check that can't finish fails again every time it's restarted: logged once until one works. */
const checkFailures = new LogOnceGate()

/** The review Apply is working through. Checks stay out of its way meanwhile: a recheck would
 *  replace the one Apply is waiting on, and Apply's outcome rechecks whatever it leaves behind. */
let applyingReview: BulkRenameReview | null = null
/** Stop checking was pressed during this Apply. Covers the gap between two batches, when no check
 *  is running for the backend to stop. */
let applyStopRequested = false

type ProposalSnapshot = Extract<AskCmdrStreamEvent, { type: 'proposalReady' }>['proposal']

/**
 * The plans this turn has staged but not shown yet.
 *
 * The review opens on the turn boundary, so the user meets a 500-file job once rather than
 * once per batch. Plain module state: nothing renders from it, and it lives exactly as long as
 * the turn does.
 */
let stagedProposals: ProposalSnapshot[] = []

/** Stage one batch. It joins the review the current turn is building, and shows on turn end. */
export function stageRenameProposal(proposal: ProposalSnapshot): void {
  stagedProposals.push(proposal)
}

/**
 * Show everything this turn staged, as one review.
 *
 * Called on every way a turn can end, including a failure and the user's own Stop: the plans
 * exist on the spine either way, and a staged plan the user never sees is a plan they can't
 * answer.
 */
export function openStagedRenameReview(): void {
  const staged = stagedProposals
  stagedProposals = []
  if (staged.length === 0) return
  const proposals = staged.map((proposal): BulkRenameReviewProposal => ({
    proposalId: proposal.proposalId,
    rows: proposal.rows.map((row) => ({
      ...row,
      allowed: true,
      blockedReason: null,
      warnings: [],
      nameRejected: false,
    })),
    preflighting: false,
    expired: false,
    revision: proposal.revision,
    changed: false,
    checkFailed: false,
    checkProgress: null,
    requestVersion: 0,
  }))
  const review = askCmdrState.renameReview
  // A review already on screen GROWS rather than being replaced: replacing it cancelled a plan
  // the user was reading, and asked them the same question again.
  if (review) review.proposals.push(...proposals)
  else askCmdrState.renameReview = { proposals, notice: null }
  for (const proposal of proposals) void refreshRenamePreflight(proposal.proposalId)
}

/** Drop what this turn staged without showing it: the thread it belonged to is gone. */
export function discardStagedRenameProposals(): void {
  const staged = stagedProposals
  stagedProposals = []
  for (const proposal of staged) void cancelBulkRenameProposal(proposal.proposalId)
}

/** The proposal with this id, as it stands NOW, or `null` if the review moved on without it. */
function liveProposal(proposalId: string): BulkRenameReviewProposal | null {
  return askCmdrState.renameReview?.proposals.find((candidate) => candidate.proposalId === proposalId) ?? null
}

/** The row as it stands NOW, or `null` if the review closed or was replaced meanwhile. */
function liveRenameRow(proposalId: string, rowId: string): BulkRenameReviewRow | null {
  return liveProposal(proposalId)?.rows.find((candidate) => candidate.rowId === rowId) ?? null
}

/** Every row on show, whichever proposal staged it. */
function allRenameRows(): BulkRenameReviewRow[] {
  return askCmdrState.renameReview?.proposals.flatMap((proposal) => proposal.rows) ?? []
}

/** Change one row's user decision, then revalidate its proposal's exact allowed subset. */
export function setRenameRowAllowed(proposalId: string, rowId: string, allowed: boolean): void {
  const row = liveRenameRow(proposalId, rowId)
  if (!row || (row.blockedReason && allowed)) return
  row.allowed = allowed
  void recheckUnlessStopped(proposalId)
}

/**
 * Replace one row's proposed name with the one the user typed, then revalidate.
 *
 * The backend owns everything about the new name: it validates it, swaps the row's evidence for
 * the "you typed this" marker (the model's quote described the model's name), and invalidates the
 * accepted preflight — so the fresh preflight below is what lets the edited name be applied at
 * all. A name it won't take leaves the row on the name it had, said plainly on the row.
 */
export async function reviseRenameRow(proposalId: string, rowId: string, destinationName: string): Promise<void> {
  const proposal = liveProposal(proposalId)
  const row = proposal?.rows.find((candidate) => candidate.rowId === rowId)
  if (!proposal || !row || proposal.expired) return
  // No IPC for a field the user left as it was (a blur after no edit, or Enter twice).
  if (destinationName === row.destinationName) {
    row.nameRejected = false
    return
  }
  try {
    const revised = await reviseBulkRenameRow(proposalId, rowId, destinationName)
    const current = liveRenameRow(proposalId, rowId)
    if (!current) return
    current.destinationName = revised.destinationName
    current.evidence = revised.evidence
    current.coverage = revised.coverage
    current.nameRejected = false
    await recheckUnlessStopped(proposalId)
  } catch (e) {
    log.warn('revising a proposed name failed: {error}', { error: e })
    noteReviseRefusal(proposalId, rowId, bulkRenameFailureOf(e))
  }
}

/** A refused name is the row's to say; a review that's gone or a store that didn't answer isn't
 *  about the name, so "try a different one" would send the user the wrong way. */
function noteReviseRefusal(proposalId: string, rowId: string, failure: BulkRenameError): void {
  const proposal = liveProposal(proposalId)
  if (!proposal) return
  if (failure.type === 'rowRefused') {
    const row = proposal.rows.find((candidate) => candidate.rowId === rowId)
    if (row) row.nameRejected = true
  } else if (failure.type === 'reviewExpired') {
    proposal.expired = true
  } else if (askCmdrState.renameReview) {
    askCmdrState.renameReview.notice = { kind: 'nameNotSaved' }
  }
}

/** Allow every row the latest preflight did not block, across every batch. */
export function allowAllRenameRows(): void {
  for (const proposal of askCmdrState.renameReview?.proposals ?? []) {
    for (const row of proposal.rows) {
      if (!row.blockedReason) row.allowed = true
    }
    void recheckUnlessStopped(proposal.proposalId)
  }
}

/** Deny every row. This sends no filesystem request and creates no operation. */
export function denyAllRenameRows(): void {
  for (const proposal of askCmdrState.renameReview?.proposals ?? []) {
    for (const row of proposal.rows) row.allowed = false
    void recheckUnlessStopped(proposal.proposalId)
  }
}

/** Revalidates the batches a review holds when the pane's existing file watcher reports a
 * name that participates in one of them. The backend remains authoritative; this name filter
 * only avoids unrelated watcher traffic causing extra IPC, and keeps a job spanning several
 * folders from re-preflighting every batch over one folder's change. */
export async function renameReviewListingChanged(
  changes: ReadonlyArray<{ type?: string; entry: { name: string } }>,
): Promise<void> {
  const changed = new SvelteSet(changes.map((change) => change.entry.name))
  const affected = (askCmdrState.renameReview?.proposals ?? []).filter((proposal) =>
    proposal.rows.some((row) => changed.has(row.sourceName) || changed.has(row.destinationName)),
  )
  await Promise.all(affected.map((proposal) => recheckUnlessStopped(proposal.proposalId)))
}

/** Restart every check that didn't finish: the Check again button. */
export function checkRenameReviewAgain(): void {
  for (const proposal of askCmdrState.renameReview?.proposals ?? []) {
    if (isUnchecked(proposal)) void refreshRenamePreflight(proposal.proposalId)
  }
}

/** Stop every running check, keeping the review open: the Stop checking button. */
export function stopRenameChecks(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  if (applyingReview === review) applyStopRequested = true
  for (const proposal of review.proposals) {
    if (!proposal.preflighting) continue
    cancelBulkRenameCheck(proposal.proposalId).catch((e: unknown) => {
      log.warn('stopping the rename check failed: {error}', { error: e })
    })
  }
}

/** The backend's word on a running check. A late one for a check that already ended is dropped. */
export function renameCheckProgressed(progress: BulkRenameCheckProgress): void {
  const proposal = liveProposal(progress.proposalId)
  if (proposal?.preflighting) proposal.checkProgress = { checked: progress.checked, total: progress.total }
}

/** The destination names in this review the user did NOT take: denied rows, and (when the whole
 * review is cancelled) every row. Names only — what the model needs is the fact that a style was
 * rejected, and a reason would be its own words handed back to it.
 *
 * A job is answered once now, so there is no later batch in the same job to teach. What this
 * still feeds is the user's NEXT message ("not like that, try dates instead"), which is why it
 * survives the grouping. */
function rememberDeniedNames(rows: { allowed: boolean; destinationName: string }[]): void {
  const denied = rows.filter((row) => !row.allowed).map((row) => row.destinationName)
  if (denied.length === 0) return
  // Newest first, so the cap in the envelope keeps the most recent decision.
  askCmdrState.deniedNames = [...denied, ...askCmdrState.deniedNames]
}

/** Cancel closes the review and consumes every proposal it holds. */
export function cancelRenameReview(): void {
  if (!askCmdrState.renameReview) return
  // Cancelling turns down every row, and that is exactly the feedback the next try needs.
  rememberDeniedNames(allRenameRows().map((row) => ({ allowed: false, destinationName: row.destinationName })))
  closeRenameReview()
}

/**
 * Start one managed operation per batch, for the rows the user currently allows.
 *
 * Sequential, in staging order, because that is the order undo has to reverse: a later batch
 * can have taken a name an earlier one freed. A batch that fails leaves itself and everything
 * after it in the review, revalidated, so the user can answer what is left.
 */
export async function applyRenameReview(): Promise<void> {
  const review = askCmdrState.renameReview
  if (!review || review.proposals.some((proposal) => proposal.preflighting || isUnchecked(proposal))) return
  const pending = review.proposals
    .filter((proposal) => !proposal.expired && !proposal.changed)
    .map((proposal) => ({
      proposalId: proposal.proposalId,
      revision: proposal.revision,
      allowedRowIds: proposal.rows.filter((row) => row.allowed && !row.blockedReason).map((row) => row.rowId),
    }))
    .filter((batch) => batch.allowedRowIds.length > 0)
  if (pending.length === 0) return
  // Read the decisions before the first batch leaves the review, so a run that stops partway
  // still reports what the user turned down in the batches that did start.
  const decisions = allRenameRows().map((row) => ({ allowed: row.allowed, destinationName: row.destinationName }))
  review.notice = null
  for (const proposal of review.proposals) proposal.preflighting = true
  applyingReview = review
  applyStopRequested = false
  let applied = 0
  let stopped = false
  for (const batch of pending) {
    // Closing the review or pressing Stop checking between two batches: there's no check running
    // for the backend to stop, so this is what keeps the next batch from starting.
    if (applyShouldStop(review)) {
      stopped = true
      break
    }
    try {
      const started = await applyBulkRename(batch.proposalId, batch.revision, batch.allowedRowIds)
      applied += 1
      const swapsSkipped = started.swapsLeftOut
      noteRenameApplied(started.operationId, batch.allowedRowIds.length - swapsSkipped, swapsSkipped)
      dropAppliedProposal(batch.proposalId)
    } catch (e) {
      const failure = bulkRenameFailureOf(e)
      stopped = failure.type === 'cancelled'
      noteApplyRefusal(batch.proposalId, failure)
      break
    }
  }
  applyingReview = null
  // Applying a subset is a decision about the rest: carry those names into the next message.
  if (applied > 0) rememberDeniedNames(decisions)
  if (applied === pending.length) {
    // Every batch the user allowed something in has started. A batch still here is one they
    // turned down whole, which is an answer too, so it is consumed rather than left on screen.
    closeRenameReview()
    return
  }
  // A batch refused to start, so the rest of the job is still the user's to answer and goes
  // back to a checked state. Unless the user stopped it: then they've asked for less checking.
  for (const proposal of askCmdrState.renameReview?.proposals ?? []) {
    finishCheck(proposal)
    if (!stopped) void refreshRenamePreflight(proposal.proposalId)
  }
}

/** Whether Apply's next batch should stay unstarted: Stop checking was pressed, or the review it
 *  belongs to has closed. */
function applyShouldStop(review: BulkRenameReview): boolean {
  return applyStopRequested || askCmdrState.renameReview !== review
}

/** Mark what an Apply refusal means on the batch it hit, or above the list when no mark says it.
 *  Exhaustive, so a new refusal is a compile error until it's decided. */
function noteApplyRefusal(proposalId: string, failure: BulkRenameError): void {
  const review = askCmdrState.renameReview
  const proposal = liveProposal(proposalId)
  if (!review || !proposal) return
  if (failure.type !== 'listChanged' && failure.type !== 'cancelled') {
    log.warn('starting the rename plan failed: {error}', { error: failure.type })
  }
  switch (failure.type) {
    case 'listChanged':
      // The agent amended this batch since the user read it: nothing ran, and its rows stay put.
      proposal.changed = true
      return
    case 'reviewExpired':
      proposal.expired = true
      return
    case 'cancelled':
      proposal.checkFailed = true
      return
    case 'needsAnotherLook':
      review.notice = { kind: 'needsAnotherLook' }
      return
    case 'nothingToApply':
      review.notice = { kind: 'nothingToApply' }
      return
    case 'couldntStart':
      review.notice =
        failure.reason.type === 'nothingToRename'
          ? { kind: 'nothingToApply' }
          : { kind: 'couldntStart', reason: failure.reason }
      return
    case 'rowRefused':
    case 'unexpected':
      review.notice = { kind: 'couldntStart', reason: null }
      return
  }
}

/** Close the review, consuming every proposal still in it. */
function closeRenameReview(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  askCmdrState.renameReview = null
  for (const proposal of review.proposals) void cancelBulkRenameProposal(proposal.proposalId)
}

/** Take a started batch out of the review; the rest of the job is still on screen. */
function dropAppliedProposal(proposalId: string): void {
  const review = askCmdrState.renameReview
  if (!review) return
  review.proposals = review.proposals.filter((proposal) => proposal.proposalId !== proposalId)
}

/**
 * Record a finished batch in the thread, with its undo.
 *
 * The line goes in the thread rather than in the (now closed) review dialog,
 * because that's where the user is looking and because a run of batches then reads
 * as a run. **Only the newest line carries the job-wide undo**, and only once a run
 * has more than one batch: the previous lines hand their ids over and keep just
 * their own Undo, so "undo everything" appears once, at the bottom. That holds
 * whether the ids arrive one turn at a time or together from one Apply.
 */
export function noteRenameApplied(operationId: string, fileCount: number, swapsSkipped = 0): void {
  // With the operation log off the batch wasn't journaled: no Undo of its own, and it
  // stays out of the run's job-wide undo, which keeps covering what was recorded.
  if (!isOperationLogRecording()) {
    askCmdrState.messages.push({
      kind: 'renameApplied',
      operationId,
      fileCount,
      jobOperationIds: [],
      jobFileCount: 0,
      swapsSkipped,
      undo: { status: 'notRecorded' },
    })
    return
  }
  const run = renameRunLines()
  // Built from the lines themselves, never from a previous line's stored job set:
  // that set already includes its predecessors, so folding it in would repeat ids.
  const jobOperationIds = [...run.map((line) => line.operationId), operationId]
  const jobFileCount = run.reduce((total, line) => total + line.fileCount, 0) + fileCount
  // The older lines are no longer the newest, so they give up the job-wide action.
  for (const line of run) {
    line.jobOperationIds = []
    line.jobFileCount = 0
  }
  askCmdrState.messages.push({
    kind: 'renameApplied',
    operationId,
    fileCount,
    jobOperationIds: jobOperationIds.length > 1 ? jobOperationIds : [],
    jobFileCount: jobOperationIds.length > 1 ? jobFileCount : 0,
    swapsSkipped,
    undo: { status: 'undoable' },
  })
}

/** Every rename line in this thread that can still be undone, oldest first. */
function renameRunLines(): Extract<RailMessage, { kind: 'renameApplied' }>[] {
  return askCmdrState.messages.filter(
    (message): message is Extract<RailMessage, { kind: 'renameApplied' }> =>
      message.kind === 'renameApplied' && message.undo.status === 'undoable',
  )
}

/**
 * Put the old names back for one batch, or (`scope: 'job'`) for every batch of the
 * run this line closes.
 *
 * The ids go over in APPLY order; the backend reverses them newest first, which is
 * the only order that works when a later batch took a name an earlier one freed. It
 * resolves when the reversal has actually finished, so the line can report what
 * came back rather than claiming success on dispatch.
 */
export async function undoRename(
  line: Extract<RailMessage, { kind: 'renameApplied' }>,
  scope: 'batch' | 'job' = 'batch',
): Promise<void> {
  if (line.undo.status !== 'undoable') return
  const operationIds = scope === 'job' && line.jobOperationIds.length > 0 ? line.jobOperationIds : [line.operationId]
  const covered = renameRunLines().filter((candidate) => operationIds.includes(candidate.operationId))
  // Remember each covered line's state so a call that never reached the backend can
  // hand its Undo back. A plain array of pairs, since this map is local and never
  // read reactively (`SvelteMap` would be reactivity nobody observes).
  const previous = covered.map((candidate) => [candidate, candidate.undo] as const)
  for (const candidate of covered) candidate.undo = { status: 'undoing' }
  try {
    const report = await undoOperations(operationIds)
    const state = undoStateFromReport(report)
    // The line the user clicked reports the whole tally; the others it covered are
    // done with, so they stop offering an Undo that would now be refused.
    line.undo = state
    for (const candidate of covered) {
      if (candidate !== line) candidate.undo = { status: 'unavailable' }
      candidate.jobOperationIds = []
      candidate.jobFileCount = 0
    }
  } catch (e) {
    log.warn('undoing the rename failed: {error}', { error: e })
    // Nothing is known to have moved, so hand every line its Undo back.
    for (const [candidate, state] of previous) candidate.undo = state
  }
}

/** Close the review and consume every proposal it holds, plus anything staged but unshown. */
export function discardRenameReview(): void {
  discardStagedRenameProposals()
  closeRenameReview()
}

/** The typed refusal behind a caught value. Anything else (the IPC call itself breaking) has no
 *  variant of its own, and reads as what it is: unexpected. */
function bulkRenameFailureOf(error: unknown): BulkRenameError {
  return failureOf(BulkRenameFailure, error) ?? { type: 'unexpected', detail: String(error) }
}

/** A batch's check is over, however it ended. The progress readout goes when the last one ends. */
function finishCheck(proposal: BulkRenameReviewProposal): void {
  proposal.preflighting = false
  if (proposal.checkProgress) proposal.checkProgress.checked = proposal.checkProgress.total
  const proposals = askCmdrState.renameReview?.proposals ?? []
  if (proposals.some((candidate) => candidate.preflighting)) return
  for (const candidate of proposals) candidate.checkProgress = null
}

/** Recheck after a change, unless this batch's check didn't finish. Restarting that one is the
 *  user's call: on a slow volume, every toggle would otherwise restart the same long check. */
async function recheckUnlessStopped(proposalId: string): Promise<void> {
  if (!liveProposal(proposalId)?.checkFailed) await refreshRenamePreflight(proposalId)
}

async function refreshRenamePreflight(proposalId: string): Promise<void> {
  const proposal = liveProposal(proposalId)
  if (!proposal || applyingReview === askCmdrState.renameReview) return
  const version = proposal.requestVersion + 1
  proposal.requestVersion = version
  proposal.preflighting = true
  proposal.checkFailed = false
  // Validate every displayed row, including denied and previously blocked rows.
  // Otherwise a target that disappears after blocking its row could never make
  // that row reviewable again. Apply still submits only the user's allowed ids.
  const allowedRowIds = proposal.rows.map((row) => row.rowId)
  try {
    const result = await preflightBulkRename(proposalId, proposal.revision, allowedRowIds)
    const current = liveProposal(proposalId)
    if (!current || current.requestVersion !== version) return
    checkFailures.clear()
    finishCheck(current)
    takeCheckAnswer(current, result)
  } catch (e) {
    const current = liveProposal(proposalId)
    if (!current || current.requestVersion !== version) return
    finishCheck(current)
    current.checkFailed = true
    const failure = bulkRenameFailureOf(e)
    // A stop is the user's own choice, so there's nothing to report.
    if (failure.type !== 'cancelled' && checkFailures.shouldLog(failure.type)) {
      log.warn('checking the rename plan failed: {error}', { error: failure.type })
    }
  }
}

/** Write a finished check's answer onto its batch. */
function takeCheckAnswer(
  proposal: BulkRenameReviewProposal,
  result: Awaited<ReturnType<typeof preflightBulkRename>>,
): void {
  proposal.expired = result.status === 'expired'
  if (proposal.expired) return
  if (result.status === 'listChanged') {
    proposal.changed = true
    return
  }
  for (const row of proposal.rows) {
    const backend = result.rows.find((candidate) => candidate.rowId === row.rowId)
    row.blockedReason = backend?.status === 'blocked' ? backend.reason : null
    if (backend) row.warnings = backend.warnings
    if (row.blockedReason) row.allowed = false
  }
}
