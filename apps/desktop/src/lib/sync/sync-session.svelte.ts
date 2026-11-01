/**
 * One open Synchronize folders dialog's session: the options, the compare it runs in the backend, and the grid rows
 * it pages from there. The backend holds the plan (`src-tauri/src/sync/`); this holds what the dialog shows of it.
 *
 * - Only the newest compare speaks: events of another session or an older generation are ignored (an event that
 *   arrives before `startSyncCompare` answers waits for it).
 * - Options never re-compare on their own: a change marks the grid `stale`. Show-filters never re-compare either;
 *   they re-page the same plan.
 * - Every plan change (an action, a content answer, `sync-rows-changed`) re-reads the summary and re-pages the rows
 *   the grid shows; pages are cut per plan revision on the backend, so this stays cheap.
 *
 * IPC arrives through `deps.ipc` (the `#lib/tauri-commands` wrappers in the app, fakes in tests).
 */
import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  asProfileEditRefusal,
  asSyncError,
  type BulkAction,
  type CompareRequest,
  type CompareStarted,
  type Mode,
  type ProfileEditRefusal,
  type RootRef,
  type ShowFilters,
  type SyncCompareFinished,
  type SyncCompareProgress,
  type SyncDefaults,
  type SyncError,
  type SyncOptions,
  type SyncRowsChanged,
  type SyncRowsPage,
  type SyncState,
  type ViewRowWire,
} from '#lib/tauri-commands/sync.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { DEFAULT_SHOW_FILTERS, NO_OVERRIDES, defaultSyncOptions, type SyncOptionKey } from './sync-options'
import { withoutSelections, type SyncRoots } from './pane-roots'

const log = getAppLogger('sync')

/** Rows per page asked of the backend. */
export const SYNC_PAGE_SIZE = 200
/** Rows per request when the dialog needs the whole view (⌘A, ⌘C on a big selection). */
const WHOLE_VIEW_CHUNK = 2000

export interface SyncIpc {
  startSyncCompare: (request: CompareRequest) => Promise<CompareStarted>
  setSyncComparePaused: (sessionId: string, paused: boolean) => Promise<void>
  cancelSyncCompare: (sessionId: string) => Promise<void>
  // eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the `#lib/tauri-commands/sync` wrapper it carries
  getSyncRows: (sessionId: string, start: number, end: number, filters: ShowFilters) => Promise<SyncRowsPage>
  getSyncState: (sessionId: string, filters: ShowFilters) => Promise<SyncState>
  setSyncActions: (sessionId: string, items: number[], bulk: BulkAction, filters: ShowFilters) => Promise<SyncState>
  compareSyncItemContent: (sessionId: string, item: number) => Promise<void>
  closeSyncSession: (sessionId: string) => Promise<void>
  // eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the `#lib/tauri-commands/sync` wrapper it carries
  getSyncLastOptions: (left: RootRef, right: RootRef) => Promise<SyncOptions | null>
  // eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the `#lib/tauri-commands/sync` wrapper it carries
  saveSyncLastOptions: (left: RootRef, right: RootRef, options: SyncOptions) => Promise<void>
  onSyncCompareProgress: (handler: (payload: SyncCompareProgress) => void) => Promise<UnlistenFn>
  onSyncCompareFinished: (handler: (payload: SyncCompareFinished) => void) => Promise<UnlistenFn>
  onSyncRowsChanged: (handler: (payload: SyncRowsChanged) => void) => Promise<UnlistenFn>
}

export interface SyncSessionDeps {
  ipc: SyncIpc
  roots: SyncRoots
  /** The Synchronize folders Settings, read at each compare. */
  readDefaults: () => SyncDefaults
  /** The pane setting: hidden files go along when the panes show them. */
  includeHidden: () => boolean
}

export type SyncPhase = 'idle' | 'comparing' | 'paused' | 'done' | 'stopped' | 'refused'

/** A refusal the dialog shows: the backend's typed one, or something it didn't classify (logged). */
export type SyncProblem = SyncError | { type: 'unexpected' }

function problemOf(error: unknown, what: string): SyncProblem {
  const typed = asSyncError(error)
  if (typed) return typed
  log.warn('{what} went wrong: {error}', { what, error })
  return { type: 'unexpected' }
}

class SyncSessionState {
  roots = $state.raw<SyncRoots>({} as SyncRoots)
  options = $state.raw<SyncOptions>(defaultSyncOptions())
  filters = $state.raw<ShowFilters>({ ...DEFAULT_SHOW_FILTERS })
  phase = $state<SyncPhase>('idle')
  progress = $state.raw<SyncCompareProgress | null>(null)
  error = $state.raw<SyncProblem | null>(null)
  /** An option changed since the compare the grid shows. */
  optionsChanged = $state(false)
  state = $state.raw<SyncState | null>(null)
  total = $state(0)
  sessionId = $state<string | null>(null)
  /** The mode the plan on screen was built with (the options may have moved on since). */
  comparedMode = $state<Mode>('bothWays')
  readonly rows = new SvelteMap<number, ViewRowWire>()
  /** Plan indices of the rows the person selected. */
  readonly selected = new SvelteSet<number>()

  private generation = 0
  private viewVersion = 0
  private readonly loaded = new Set<number>()
  private lastRange: { start: number; end: number } | null = null
  private unlisten: UnlistenFn[] = []
  /** A finished event that came before `startSyncCompare` told us its generation. */
  private early: SyncCompareFinished[] = []

  constructor(private readonly deps: SyncSessionDeps) {
    this.roots = deps.roots
  }

  /** The grid needs a new compare: an option changed, or a run changed the folders. */
  get stale(): boolean {
    return this.optionsChanged || this.state?.stale === true
  }

  async open(): Promise<void> {
    const { ipc } = this.deps
    this.unlisten = await Promise.all([
      ipc.onSyncCompareProgress((payload) => {
        this.onProgress(payload)
      }),
      ipc.onSyncCompareFinished((payload) => {
        this.onFinished(payload)
      }),
      ipc.onSyncRowsChanged((payload) => {
        if (payload.sessionId === this.sessionId) this.refresh()
      }),
    ])
    try {
      const last = await ipc.getSyncLastOptions(this.roots.left.root, this.roots.right.root)
      if (last) this.options = last
    } catch (error) {
      log.warn("couldn't read the remembered options: {error}", { error })
    }
    await this.compare()
  }

  async compare(): Promise<void> {
    const roots = this.options.onlySelected ? this.roots : withoutSelections(this.roots)
    const request: CompareRequest = {
      sessionId: this.sessionId,
      left: roots.left,
      right: roots.right,
      options: this.options,
      defaults: this.deps.readDefaults(),
      overrides: { ...NO_OVERRIDES },
      profileName: null,
      includeHidden: this.deps.includeHidden(),
    }
    this.phase = 'comparing'
    this.error = null
    this.progress = null
    try {
      const started = await this.deps.ipc.startSyncCompare(request)
      this.sessionId = started.sessionId
      this.generation = started.generation
      this.comparedMode = request.options.mode
      this.optionsChanged = false
      const early = this.early.find((event) => this.isCurrent(event))
      this.early = []
      if (early) this.onFinished(early)
    } catch (error) {
      this.phase = 'refused'
      this.error = problemOf(error, 'starting a compare')
    }
  }

  private isCurrent(event: { sessionId: string; generation: number }): boolean {
    return event.sessionId === this.sessionId && event.generation === this.generation
  }

  private onProgress(payload: SyncCompareProgress): void {
    if (this.isCurrent(payload)) this.progress = payload
  }

  private onFinished(payload: SyncCompareFinished): void {
    if (!this.isCurrent(payload)) {
      // Our compare may not have answered yet: keep it for when it does.
      if (this.phase === 'comparing' && payload.generation > this.generation) this.early = [...this.early, payload]
      return
    }
    const { outcome } = payload
    if (outcome.type === 'refused') {
      this.phase = 'refused'
      this.error = outcome.error
      return
    }
    this.phase = outcome.type === 'done' ? 'done' : 'stopped'
    this.selected.clear()
    this.refresh()
  }

  /** Re-reads the summary and re-pages what the grid shows. */
  refresh(): void {
    const id = this.sessionId
    if (id === null) return
    void this.deps.ipc.getSyncState(id, this.filters).then(
      (state) => {
        this.state = state
      },
      (error: unknown) => {
        const problem = problemOf(error, 'reading the summary')
        // A stopped first compare has no plan yet: nothing to show, nothing to say.
        if (problem.type !== 'notCompared') this.error = problem
        this.state = null
      },
    )
    this.invalidate()
  }

  private invalidate(): void {
    this.viewVersion++
    this.loaded.clear()
    const range = this.lastRange ?? { start: 0, end: SYNC_PAGE_SIZE }
    this.ensureRange(range.start, range.end)
  }

  /** The grid scrolled: load the pages that cover `start..end`. */
  ensureRange(start: number, end: number): void {
    this.lastRange = { start, end }
    const first = Math.floor(start / SYNC_PAGE_SIZE)
    const last = Math.floor(Math.max(start, end - 1) / SYNC_PAGE_SIZE)
    for (let page = first; page <= last; page++) {
      if (!this.loaded.has(page)) void this.loadPage(page)
    }
  }

  private async loadPage(page: number): Promise<void> {
    const id = this.sessionId
    if (id === null) return
    this.loaded.add(page)
    const version = this.viewVersion
    try {
      const answer = await this.deps.ipc.getSyncRows(
        id,
        page * SYNC_PAGE_SIZE,
        (page + 1) * SYNC_PAGE_SIZE,
        this.filters,
      )
      if (version !== this.viewVersion) return
      this.applyPage(answer)
    } catch (error) {
      if (version !== this.viewVersion) return
      this.loaded.delete(page)
      const problem = problemOf(error, 'reading grid rows')
      if (problem.type !== 'notCompared') this.error = problem
    }
  }

  private applyPage(answer: SyncRowsPage): void {
    if (answer.total !== this.total) {
      for (const index of [...this.rows.keys()]) if (index >= answer.total) this.rows.delete(index)
      this.total = answer.total
    }
    for (const { viewIndex, row } of answer.rows) this.rows.set(viewIndex, row)
  }

  getRow(viewIndex: number): ViewRowWire | undefined {
    return this.rows.get(viewIndex)
  }

  /** The whole view, in order, for ⌘A and for copying a big selection. */
  async allRows(): Promise<ViewRowWire[]> {
    const id = this.sessionId
    if (id === null) return []
    const out: ViewRowWire[] = []
    for (let start = 0; start < Math.max(this.total, 1); start += WHOLE_VIEW_CHUNK) {
      const answer = await this.deps.ipc.getSyncRows(id, start, start + WHOLE_VIEW_CHUNK, this.filters)
      out.push(...answer.rows.map((entry) => entry.row))
      if (answer.total <= start + WHOLE_VIEW_CHUNK) break
    }
    return out
  }

  /** A view row, loading its page when it isn't here yet. */
  async rowAt(viewIndex: number): Promise<ViewRowWire | undefined> {
    const cached = this.rows.get(viewIndex)
    if (cached || this.sessionId === null) return cached
    const page = Math.floor(viewIndex / SYNC_PAGE_SIZE)
    this.loaded.delete(page)
    await this.loadPage(page)
    return this.rows.get(viewIndex)
  }

  async setPaused(paused: boolean): Promise<void> {
    if (this.sessionId === null) return
    try {
      await this.deps.ipc.setSyncComparePaused(this.sessionId, paused)
      this.phase = paused ? 'paused' : 'comparing'
    } catch (error) {
      this.error = problemOf(error, 'pausing the compare')
    }
  }

  async stop(): Promise<void> {
    if (this.sessionId === null) return
    try {
      await this.deps.ipc.cancelSyncCompare(this.sessionId)
    } catch (error) {
      this.error = problemOf(error, 'stopping the compare')
    }
  }

  setOption<K extends SyncOptionKey>(key: K, value: SyncOptions[K]): void {
    if (this.options[key] === value) return
    this.options = { ...this.options, [key]: value }
    if (this.sessionId !== null) this.optionsChanged = true
  }

  setFilters(next: ShowFilters): void {
    this.filters = next
    this.refresh()
  }

  /** ⌘U: the sides trade places, and the dialog compares again. */
  async swap(): Promise<void> {
    this.roots = { left: this.roots.right, right: this.roots.left }
    await this.compare()
  }

  /** One key's action on `items` (plan indices); the backend keeps only the ones the filters show. */
  async setActions(items: number[], bulk: BulkAction): Promise<SyncProblem | null> {
    if (this.sessionId === null || items.length === 0) return null
    try {
      this.state = await this.deps.ipc.setSyncActions(this.sessionId, items, bulk, this.filters)
      this.invalidate()
      return null
    } catch (error) {
      return problemOf(error, 'changing actions')
    }
  }

  /** Enter on a row: compare the pair byte by byte; `sync-rows-changed` brings the answer. */
  async compareContent(item: number): Promise<SyncProblem | null> {
    if (this.sessionId === null) return null
    try {
      await this.deps.ipc.compareSyncItemContent(this.sessionId, item)
      this.invalidate()
      return null
    } catch (error) {
      return problemOf(error, 'comparing content')
    }
  }

  /** Remembers the options for this pair and ends the session. Answers why the options weren't saved, if they weren't. */
  async close(): Promise<ProfileEditRefusal | null> {
    for (const stop of this.unlisten) stop()
    this.unlisten = []
    let refusal: ProfileEditRefusal | null = null
    try {
      await this.deps.ipc.saveSyncLastOptions(this.roots.left.root, this.roots.right.root, this.options)
    } catch (error) {
      refusal = asProfileEditRefusal(error)
      if (!refusal) log.warn("couldn't remember the options: {error}", { error })
    }
    if (this.sessionId !== null) {
      try {
        await this.deps.ipc.closeSyncSession(this.sessionId)
      } catch (error) {
        log.warn("couldn't close the session: {error}", { error })
      }
    }
    return refusal
  }
}

export type SyncSession = SyncSessionState

export function createSyncSession(deps: SyncSessionDeps): SyncSession {
  return new SyncSessionState(deps)
}
