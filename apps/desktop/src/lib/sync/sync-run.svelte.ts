/**
 * The confirm, the run, and its result for one dialog (spec 4.5). Nothing runs without the confirm: `accept()` sends
 * exactly the preview's revision and the Settings it was shown with, so a plan that changed since is refused by the
 * backend (`planChanged`) and shown again, never run unseen.
 *
 * Phases: `idle` (the grid) → `confirming` (the preview is up) → `running` / `paused` → `finished` (the result).
 * Dismissing the result compares again (TC's `SyncRecompareAfterCopy`): a run leaves the compare stale.
 */
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getAppLogger } from '#lib/logging/logger.js'
import {
  asSyncError,
  type Acknowledgement,
  type ConfirmGroupKey,
  type ConfirmPreview,
  type RunProgress,
  type RunReportWire,
  type RunRequest,
  type ShowFilters,
  type SyncDefaults,
  type SyncRunFinished,
  type SyncRunProgress,
} from '#lib/tauri-commands/sync.js'
import { isHardHeld } from './confirm-rules'
import { notDoneItems } from './run-result'
import type { SyncProblem } from './sync-session.svelte'

const log = getAppLogger('sync')

export interface SyncRunIpc {
  getSyncConfirm: (sessionId: string, defaults: SyncDefaults, filters: ShowFilters) => Promise<ConfirmPreview>
  startSyncRun: (sessionId: string, request: RunRequest) => Promise<void>
  setSyncRunPaused: (sessionId: string, paused: boolean) => Promise<void>
  cancelSyncRun: (sessionId: string) => Promise<void>
  onSyncRunProgress: (handler: (payload: SyncRunProgress) => void) => Promise<UnlistenFn>
  onSyncRunFinished: (handler: (payload: SyncRunFinished) => void) => Promise<UnlistenFn>
}

/** The parts of the dialog's session a run reads and drives. */
export interface SyncRunSession {
  readonly sessionId: string | null
  readonly filters: ShowFilters
  compare: () => Promise<void>
  refresh: () => void
}

export interface SyncRunDeps {
  ipc: SyncRunIpc
  session: SyncRunSession
  readDefaults: () => SyncDefaults
  now?: () => number
}

export type SyncRunPhase = 'idle' | 'confirming' | 'running' | 'paused' | 'finished'

/** Why the confirm on screen is a fresh one, or why it can't run. */
export type ConfirmNotice = 'planChanged' | 'staleCompare' | null

function problemOf(error: unknown, what: string): SyncProblem {
  const typed = asSyncError(error)
  if (typed) return typed
  log.warn('{what} went wrong: {error}', { what, error })
  return { type: 'unexpected' }
}

class SyncRunState {
  phase = $state<SyncRunPhase>('idle')
  preview = $state.raw<ConfirmPreview | null>(null)
  /** The groups left ticked (all, when the confirm opens). */
  accepted = $state.raw<ConfirmGroupKey[]>([])
  acknowledged = $state.raw<Acknowledgement[]>([])
  notice = $state<ConfirmNotice>(null)
  problem = $state.raw<SyncProblem | null>(null)
  progress = $state.raw<RunProgress | null>(null)
  report = $state.raw<RunReportWire | null>(null)
  /** A pause asked for lands between steps: the run says so until it does. */
  pauseRequested = $state(false)
  /** A step ended since the pause was asked for, so the run is truly resting. */
  private pauseLanded = $state(false)
  /** Cancel was asked for: the run stops after the step under way. */
  cancelling = $state(false)
  /** The confirm is fetching or the run is starting: the buttons wait. */
  busy = $state(false)

  private shownWith: SyncDefaults | null = null
  private startedAt = 0
  /** Milliseconds spent paused before the current pause, and when that one began. */
  private pausedMs = 0
  private pausedSince: number | null = null
  private unlisten: UnlistenFn[] = []
  private closed = false
  /** `startSyncRun` is on its way: the run's events already count. */
  private starting = false

  constructor(private readonly deps: SyncRunDeps) {}

  async open(): Promise<void> {
    const { ipc } = this.deps
    this.unlisten = await Promise.all([
      ipc.onSyncRunProgress((payload) => {
        this.onProgress(payload)
      }),
      ipc.onSyncRunFinished((payload) => {
        this.onFinished(payload)
      }),
    ])
    if (this.closed) this.close()
  }

  close(): void {
    this.closed = true
    for (const stop of this.unlisten) stop()
    this.unlisten = []
  }

  private get sessionId(): string | null {
    return this.deps.session.sessionId
  }

  /** ⌘↩ or Synchronize: show what would run. */
  async openConfirm(): Promise<void> {
    if (this.phase !== 'idle' || this.busy) return
    this.notice = null
    this.acknowledged = []
    if (await this.fetchPreview()) this.accepted = this.releasable(this.preview?.groups.map((group) => group.key) ?? [])
  }

  /** Reads the preview with the Settings as they are now; `false` (with `problem` set) when it can't. */
  private async fetchPreview(): Promise<boolean> {
    const id = this.sessionId
    if (id === null) return false
    const defaults = this.deps.readDefaults()
    this.busy = true
    this.problem = null
    try {
      this.preview = await this.deps.ipc.getSyncConfirm(id, defaults, this.deps.session.filters)
      this.shownWith = defaults
      this.phase = 'confirming'
      return true
    } catch (error) {
      this.problem = problemOf(error, 'reading the confirm')
      return false
    } finally {
      this.busy = false
    }
  }

  /** A tick changed after "the list changed": the person is checking it, so the banner has said its piece. */
  private lookedAgain(): void {
    if (this.notice === 'planChanged') this.notice = null
  }

  /** `keys` without the groups no button releases (shown held, never ticked). */
  private releasable(keys: ConfirmGroupKey[]): ConfirmGroupKey[] {
    const preview = this.preview
    return preview ? keys.filter((key) => !isHardHeld(preview, key)) : keys
  }

  private get clock(): () => number {
    return this.deps.now ?? Date.now
  }

  toggleGroup(key: ConfirmGroupKey): void {
    if (this.preview && isHardHeld(this.preview, key)) return
    this.lookedAgain()
    this.accepted = this.accepted.includes(key) ? this.accepted.filter((each) => each !== key) : [...this.accepted, key]
  }

  /** A guard's second yes ("Remove anyway", "Copy anyway"), or taking it back. */
  toggleAcknowledgement(ack: Acknowledgement): void {
    this.lookedAgain()
    this.acknowledged = this.acknowledged.includes(ack)
      ? this.acknowledged.filter((each) => each !== ack)
      : [...this.acknowledged, ack]
  }

  dismissConfirm(): void {
    if (this.phase !== 'confirming') return
    this.phase = 'idle'
    this.preview = null
    this.notice = null
    this.problem = null
  }

  /**
   * Runs the groups left ticked of the confirm on screen. A plan that changed since shows the confirm again with
   * why; a compare a run already changed can't run at all (compare again).
   */
  async accept(): Promise<void> {
    const id = this.sessionId
    const preview = this.preview
    if (this.phase !== 'confirming' || this.busy || id === null || preview === null || this.shownWith === null) return
    // A run already changed these folders: only a new compare can make a confirm that runs.
    if (this.notice === 'staleCompare') return
    const request: RunRequest = {
      revision: preview.revision,
      groups: this.accepted,
      acknowledged: this.acknowledged,
      defaults: this.shownWith,
    }
    this.busy = true
    this.problem = null
    this.startedAt = this.clock()
    this.pausedMs = 0
    this.pausedSince = null
    this.progress = null
    this.report = null
    this.pauseRequested = false
    this.pauseLanded = false
    this.cancelling = false
    // The backend spawns the run, so its progress and even its report can arrive before this call answers.
    this.starting = true
    try {
      await this.deps.ipc.startSyncRun(id, request)
    } catch (error) {
      this.starting = false
      this.busy = false
      await this.refused(problemOf(error, 'starting the run'))
      return
    }
    this.starting = false
    this.busy = false
    // A report that came first already ended it.
    if (!this.reportArrived()) this.phase = 'running'
  }

  /** Read through a call: the phase can change while `startSyncRun` is answered. */
  private reportArrived(): boolean {
    return this.phase === 'finished'
  }

  private async refused(problem: SyncProblem): Promise<void> {
    if (problem.type === 'planChanged') {
      const accepted = this.accepted
      if (await this.fetchPreview()) {
        // Keep the person's ticks for the groups that are still there.
        const keys = this.preview?.groups.map((group) => group.key) ?? []
        this.accepted = this.releasable(keys.filter((key) => accepted.includes(key)))
        this.acknowledged = []
        this.notice = 'planChanged'
      }
      return
    }
    if (problem.type === 'staleCompare') {
      this.notice = 'staleCompare'
      return
    }
    this.problem = problem
  }

  /** "Compare again" from a confirm whose compare a run already changed. */
  async compareAgain(): Promise<void> {
    this.dismissConfirm()
    await this.deps.session.compare()
  }

  /** Pause takes effect between steps; until then the panel says it's pausing. */
  async setPaused(paused: boolean): Promise<void> {
    const id = this.sessionId
    if (id === null || (this.phase !== 'running' && this.phase !== 'paused')) return
    try {
      await this.deps.ipc.setSyncRunPaused(id, paused)
      this.pauseRequested = paused
      this.pauseLanded = false
      this.phase = paused ? 'paused' : 'running'
      if (paused) this.pausedSince ??= this.clock()
      else if (this.pausedSince !== null) {
        this.pausedMs += this.clock() - this.pausedSince
        this.pausedSince = null
      }
    } catch (error) {
      this.problem = problemOf(error, 'pausing the run')
    }
  }

  /** Stops after the step under way; until the report lands the panel says it's stopping. */
  async cancel(): Promise<void> {
    const id = this.sessionId
    if (id === null || this.cancelling || (this.phase !== 'running' && this.phase !== 'paused')) return
    this.cancelling = true
    try {
      await this.deps.ipc.cancelSyncRun(id)
    } catch (error) {
      this.cancelling = false
      this.problem = problemOf(error, 'stopping the run')
    }
  }

  /** Paused, but the step under way hasn't ended yet. */
  get pausing(): boolean {
    return this.phase === 'paused' && !this.pauseLanded
  }

  private onProgress(payload: SyncRunProgress): void {
    if (payload.sessionId !== this.sessionId) return
    if (!this.starting && this.phase !== 'running' && this.phase !== 'paused') return
    this.progress = payload.progress
    // Progress comes after each step: one that ends while paused is the last before the rest.
    if (this.phase === 'paused') this.pauseLanded = true
  }

  private onFinished(payload: SyncRunFinished): void {
    if (payload.sessionId !== this.sessionId) return
    if (!this.starting && this.phase !== 'running' && this.phase !== 'paused') return
    this.report = payload.report
    this.phase = 'finished'
    this.preview = null
    this.pauseRequested = false
    this.cancelling = false
    // The folders changed: the summary now says the compare is stale.
    this.deps.session.refresh()
  }

  /** Seconds left at the bytes-per-second so far, or `null` before there's a rate. */
  get etaSeconds(): number | null {
    const progress = this.progress
    if (!progress || progress.bytesDone <= 0 || this.phase !== 'running') return null
    const elapsed = (this.clock() - this.startedAt - this.pausedMs) / 1000
    if (elapsed <= 0) return null
    return Math.max(0, Math.round(((progress.bytesTotal - progress.bytesDone) * elapsed) / progress.bytesDone))
  }

  /** Plan indices of the rows that didn't end done, for "Show in grid". */
  get notDone(): number[] {
    return this.report ? notDoneItems(this.report) : []
  }

  /** Leaves the result and compares again; `keepGrid` leaves the stale grid up instead (Show in grid). */
  async dismissResult(keepGrid = false): Promise<void> {
    if (this.phase !== 'finished') return
    this.phase = 'idle'
    if (keepGrid) return
    this.report = null
    await this.deps.session.compare()
  }
}

export type SyncRun = SyncRunState

export function createSyncRun(deps: SyncRunDeps): SyncRun {
  return new SyncRunState(deps)
}
