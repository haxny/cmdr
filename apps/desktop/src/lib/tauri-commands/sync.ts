// Synchronize folders (⇧F12): saved profiles, the options last used per pair of roots, and the check of the
// versions folder Settings names. The work is the backend's (`src-tauri/src/sync/`, `crates/cmdr-sync`); these
// are pass-throughs.

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type BulkAction,
  type CompareRequest,
  type ConfirmPreview,
  type KeepPolicy,
  type KeepRefusal,
  type ProfileEditRefusal,
  type ProfileEntry,
  type ProfileMove,
  type ProfileSaveRefusal,
  type RootRef,
  type RunRequest,
  type ShowFilters,
  type SyncCompareFinished,
  type SyncCompareProgress,
  type SyncDefaults,
  type SyncError,
  type SyncOptions,
  type SyncProfile,
  type SyncRowsChanged,
  type SyncRowsPage,
  type SyncRunFinished,
  type SyncRunProgress,
  type SyncState,
} from '#lib/ipc/bindings.js'
import { TypedFailure, failureOf } from '#lib/ipc/typed-failure.js'

export type {
  BulkAction,
  CompareRequest,
  ConfirmPreview,
  KeepPolicy,
  KeepRefusal,
  ProfileEditRefusal,
  ProfileEntry,
  ProfileMove,
  ProfileSaveRefusal,
  RootRef,
  RunRequest,
  ShowFilters,
  SyncCompareFinished,
  SyncCompareProgress,
  SyncDefaults,
  SyncError,
  SyncOptions,
  SyncProfile,
  SyncRowsChanged,
  SyncRowsPage,
  SyncRunFinished,
  SyncRunProgress,
  SyncState,
}

/** An `Error` that still carries the backend's typed profile-save refusal. */
export class SyncProfileSaveFailure extends TypedFailure<ProfileSaveRefusal> {
  constructor(failure: ProfileSaveRefusal) {
    super(failure, `sync profile save refused: ${failure.type}`)
    this.name = 'SyncProfileSaveFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asProfileSaveRefusal(error: unknown): ProfileSaveRefusal | null {
  return failureOf(SyncProfileSaveFailure, error)
}

/** An `Error` that still carries the backend's typed refusal of a rename, delete, move, or remembered-options write. */
export class SyncProfileEditFailure extends TypedFailure<ProfileEditRefusal> {
  constructor(failure: ProfileEditRefusal) {
    super(failure, `sync profile edit refused: ${failure.type}`)
    this.name = 'SyncProfileEditFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asProfileEditRefusal(error: unknown): ProfileEditRefusal | null {
  return failureOf(SyncProfileEditFailure, error)
}

function editResult<T>(res: { status: 'ok'; data: T } | { status: 'error'; error: ProfileEditRefusal }): T {
  if (res.status === 'error') throw new SyncProfileEditFailure(res.error)
  return res.data
}

/** The saved profiles, top to bottom. One that can't be used is listed with why, never dropped. */
export async function getSyncProfiles(): Promise<ProfileEntry[]> {
  return commands.getSyncProfiles()
}

/**
 * Saves a profile at the end of the list, or over the one with its name where that stands. Answers the list after.
 * Throws a {@link SyncProfileSaveFailure} (`listFull`, `invalid`) when nothing was saved.
 */
export async function saveSyncProfile(profile: SyncProfile): Promise<ProfileEntry[]> {
  const res = await commands.saveSyncProfile(profile)
  if (res.status === 'error') throw new SyncProfileSaveFailure(res.error)
  return res.data
}

/** Deletes a profile. Answers the list after; throws a {@link SyncProfileEditFailure} when the file couldn't be written. */
export async function deleteSyncProfile(id: string): Promise<ProfileEntry[]> {
  return editResult(await commands.deleteSyncProfile(id))
}

/**
 * Renames a profile in place. Throws a {@link SyncProfileEditFailure} (`nameTaken` when another profile has the name:
 * ask, then save over it; `writeFailed`). Answers the list after.
 */
export async function renameSyncProfile(id: string, name: string): Promise<ProfileEntry[]> {
  return editResult(await commands.renameSyncProfile(id, name))
}

/** Move up / Move down: swaps a profile with its neighbor. Answers the list after. */
export async function moveSyncProfile(id: string, direction: ProfileMove): Promise<ProfileEntry[]> {
  return editResult(await commands.moveSyncProfile(id, direction))
}

/** The options the dialog last ran with for this ordered pair of roots, if it remembers any. */
export async function getSyncLastOptions(left: RootRef, right: RootRef): Promise<SyncOptions | null> {
  return commands.getSyncLastOptions(left, right)
}

/** Remembers the dialog's options for this pair of roots. */
export async function saveSyncLastOptions(left: RootRef, right: RootRef, options: SyncOptions): Promise<void> {
  editResult(await commands.saveSyncLastOptions(left, right, options))
}

/** Why the versions folder `policy` names can't be used, or `null` when it can. */
export async function checkSyncKeepPolicy(policy: KeepPolicy): Promise<KeepRefusal | null> {
  return commands.checkSyncKeepPolicy(policy)
}

// Compare and run sessions: one per open dialog. Every refusal is a typed `SyncError`.

/** An `Error` that still carries the backend's typed refusal of a compare or run command. */
export class SyncCommandFailure extends TypedFailure<SyncError> {
  constructor(failure: SyncError) {
    super(failure, `sync command refused: ${failure.type}`)
    this.name = 'SyncCommandFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asSyncError(error: unknown): SyncError | null {
  return failureOf(SyncCommandFailure, error)
}

function syncResult<T>(res: { status: 'ok'; data: T } | { status: 'error'; error: SyncError }): T {
  if (res.status === 'error') throw new SyncCommandFailure(res.error)
  return res.data
}

/**
 * Checks every input and starts a compare (in `request.sessionId`, or a new session). Answers the session id; the
 * rows are ready on `sync-compare-finished`. Throws a {@link SyncCommandFailure}.
 */
export async function startSyncCompare(request: CompareRequest): Promise<string> {
  return syncResult(await commands.startSyncCompare(request))
}

/** Pauses or resumes the session's compare. */
export async function setSyncComparePaused(sessionId: string, paused: boolean): Promise<void> {
  syncResult(await commands.setSyncComparePaused(sessionId, paused))
}

/** Stops the session's compare. */
export async function cancelSyncCompare(sessionId: string): Promise<void> {
  syncResult(await commands.cancelSyncCompare(sessionId))
}

/** Grid rows `start..end` under the show-filters. */
export async function getSyncRows(
  sessionId: string,
  start: number,
  end: number,
  filters: ShowFilters,
): Promise<SyncRowsPage> {
  return syncResult(await commands.getSyncRows(sessionId, start, end, filters))
}

/** The summary bar and the guards of the current plan. */
export async function getSyncState(sessionId: string, filters: ShowFilters): Promise<SyncState> {
  return syncResult(await commands.getSyncState(sessionId, filters))
}

/** Applies a key's action to the selected rows the person can see; answers the new state. */
export async function setSyncActions(
  sessionId: string,
  items: number[],
  bulk: BulkAction,
  filters: ShowFilters,
): Promise<SyncState> {
  return syncResult(await commands.setSyncActions(sessionId, items, bulk, filters))
}

/** Compares one equal-size pair by content; `sync-rows-changed` follows. */
export async function compareSyncItemContent(sessionId: string, item: number): Promise<void> {
  syncResult(await commands.compareSyncItemContent(sessionId, item))
}

/** What the confirm shows for the current plan. */
export async function getSyncConfirm(
  sessionId: string,
  defaults: SyncDefaults,
  filters: ShowFilters,
): Promise<ConfirmPreview> {
  return syncResult(await commands.getSyncConfirm(sessionId, defaults, filters))
}

/** Runs the accepted groups; `sync-run-progress` and `sync-run-finished` follow. */
export async function startSyncRun(sessionId: string, request: RunRequest): Promise<void> {
  syncResult(await commands.startSyncRun(sessionId, request))
}

/** Pauses or resumes the run between steps. */
export async function setSyncRunPaused(sessionId: string, paused: boolean): Promise<void> {
  syncResult(await commands.setSyncRunPaused(sessionId, paused))
}

/** Stops the run; what it set aside goes back. */
export async function cancelSyncRun(sessionId: string): Promise<void> {
  syncResult(await commands.cancelSyncRun(sessionId))
}

/** Closes the dialog's session, stopping anything it runs. */
export async function closeSyncSession(sessionId: string): Promise<void> {
  await commands.closeSyncSession(sessionId)
}

/** Live scan and content-compare counts, at most every 100 ms. */
export function onSyncCompareProgress(handler: (payload: SyncCompareProgress) => void): Promise<UnlistenFn> {
  return events.syncCompareProgress.listen((event) => {
    handler(event.payload)
  })
}

/** A compare ended: done, stopped, or refused with a typed reason. */
export function onSyncCompareFinished(handler: (payload: SyncCompareFinished) => void): Promise<UnlistenFn> {
  return events.syncCompareFinished.listen((event) => {
    handler(event.payload)
  })
}

/** Rows changed without a new compare (a row's content compare answered). */
export function onSyncRowsChanged(handler: (payload: SyncRowsChanged) => void): Promise<UnlistenFn> {
  return events.syncRowsChanged.listen((event) => {
    handler(event.payload)
  })
}

/** A run's totals after each step. */
export function onSyncRunProgress(handler: (payload: SyncRunProgress) => void): Promise<UnlistenFn> {
  return events.syncRunProgress.listen((event) => {
    handler(event.payload)
  })
}

/** A run ended, with its report. */
export function onSyncRunFinished(handler: (payload: SyncRunFinished) => void): Promise<UnlistenFn> {
  return events.syncRunFinished.listen((event) => {
    handler(event.payload)
  })
}
