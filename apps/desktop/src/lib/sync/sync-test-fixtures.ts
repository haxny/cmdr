/**
 * Test fixtures for the Synchronize folders dialog: a fake IPC whose compare finishes when told, a small view with
 * root rows, a folder header, and a warning row, and a summary with a guard. Test-only.
 */
import { vi } from 'vitest'
import type {
  ConfirmPreview,
  RunReportWire,
  SyncProfile,
  SyncCompareFinished,
  SyncCompareProgress,
  SyncRowsChanged,
  SyncRunFinished,
  SyncRunProgress,
  SyncState,
  ProfileEntry,
  ViewRowWire,
} from '#lib/tauri-commands/sync.js'
import type { SyncDialogIpc } from './sync-ipc'
import type { SyncItemRow } from './row-label'
import { DEFAULT_SHOW_FILTERS, NO_OVERRIDES, defaultSyncOptions } from './sync-options'

export function itemRow(index: number, patch: Partial<SyncItemRow> = {}): SyncItemRow {
  return {
    type: 'item',
    index,
    leftName: `file-${String(index)}.txt`,
    rightName: `file-${String(index)}.txt`,
    leftRelPath: `file-${String(index)}.txt`,
    rightRelPath: `file-${String(index)}.txt`,
    leftKind: 'file',
    rightKind: 'file',
    leftSize: 1200,
    rightSize: 1000,
    leftMtime: 1_780_000_000,
    rightMtime: 1_770_000_000,
    category: 'newerLeft',
    rung: 'sizeAndTime',
    content: 'notAsked',
    sameSize: false,
    action: 'copyRight',
    defaultAction: 'copyRight',
    olderOverwritesNewer: false,
    legal: ['copyRight', 'copyLeft', 'skip'],
    ...patch,
  }
}

/** Root rows, then one folder with a left-only row, a warning row, and an undecided one. */
export const VIEW: ViewRowWire[] = [
  itemRow(0),
  itemRow(1, {
    leftName: null,
    leftRelPath: null,
    leftKind: null,
    leftSize: null,
    leftMtime: null,
    category: 'rightOnly',
    action: 'copyLeft',
    defaultAction: 'copyLeft',
    legal: ['copyLeft', 'skip'],
  }),
  { type: 'folder', relDir: 'Photos/2024', ordinal: 1, of: 1 },
  itemRow(2, {
    leftKind: 'folder',
    leftRelPath: 'Photos/2024/file-2.txt',
    rightRelPath: null,
    rightKind: null,
    rightName: null,
    rightSize: null,
    rightMtime: null,
    category: 'leftOnly',
    rung: 'presence',
  }),
  itemRow(3, {
    leftRelPath: 'Photos/2024/file-3.txt',
    rightRelPath: 'photos/2024/file-3.txt',
    category: 'newerRight',
    olderOverwritesNewer: true,
    defaultAction: 'skip',
    rung: 'content',
    content: 'pending',
  }),
  itemRow(4, {
    leftRelPath: 'Photos/2024/file-4.txt',
    rightRelPath: 'Photos/2024/file-4.txt',
    category: 'unknown',
    action: 'undecided',
    defaultAction: 'undecided',
    legal: ['copyRight', 'copyLeft', 'skip', 'undecided'],
  }),
]

export const STATE: SyncState = {
  summary: {
    intoLeft: { copyFiles: 1, copyBytes: 1000, replaceFiles: 0, replaceBytes: 0, removals: 0, removedFolders: 0 },
    intoRight: { copyFiles: 2, copyBytes: 2400, replaceFiles: 1, replaceBytes: 1200, removals: 3, removedFolders: 0 },
    skipped: 1,
    undecided: 1,
    equal: 12,
    different: 3,
    uniqueLeft: 1,
    uniqueRight: 1,
    notComparable: 0,
    hiddenActions: 2,
    changedByYou: 1,
  },
  guards: {
    blocks: [{ type: 'maxRemove', side: 'right', removals: 61, targetFiles: 100 }],
    olderOverwritesNewer: 1,
    skew: null,
  },
  stale: false,
  revision: 1,
}

export interface FakeSyncIpc {
  ipc: SyncDialogIpc
  finish: (outcome?: SyncCompareFinished['outcome']) => void
  progress: (payload: Omit<SyncCompareProgress, 'sessionId' | 'generation'>) => void
  runProgress: (payload: SyncRunProgress['progress']) => void
  runFinished: (payload: SyncRunFinished['report']) => void
  /** The profiles the fake store answers with. */
  profiles: ProfileEntry[]
}

export function fakeSyncIpc(view: ViewRowWire[] = VIEW, state: SyncState = STATE): FakeSyncIpc {
  const handlers: {
    finished?: (p: SyncCompareFinished) => void
    progress?: (p: SyncCompareProgress) => void
    rows?: (p: SyncRowsChanged) => void
    runProgress?: (p: SyncRunProgress) => void
    runFinished?: (p: SyncRunFinished) => void
  } = {}
  let generation = 0
  const profiles: ProfileEntry[] = []
  const listen = <P>(set: (h: (p: P) => void) => void) =>
    vi.fn((h: (p: P) => void) => {
      set(h)
      return Promise.resolve(() => {})
    })
  const ipc: SyncDialogIpc = {
    startSyncCompare: vi.fn(() => Promise.resolve({ sessionId: 's1', generation: ++generation })),
    setSyncComparePaused: vi.fn(() => Promise.resolve()),
    cancelSyncCompare: vi.fn(() => Promise.resolve()),
    getSyncRows: vi.fn((_id: string, start: number, end: number) =>
      Promise.resolve({
        total: view.length,
        rows: view.slice(start, end).map((row, i) => ({ viewIndex: start + i, row })),
      }),
    ),
    getSyncState: vi.fn(() => Promise.resolve(state)),
    setSyncActions: vi.fn(() => Promise.resolve({ state, skipped: [], hidden: 0 })),
    findSyncRow: vi.fn(() => Promise.resolve(null)),
    compareSyncItemContent: vi.fn(() => Promise.resolve()),
    closeSyncSession: vi.fn(() => Promise.resolve()),
    getSyncLastOptions: vi.fn(() => Promise.resolve(null)),
    saveSyncLastOptions: vi.fn(() => Promise.resolve()),
    onSyncCompareFinished: vi.fn((h: (p: SyncCompareFinished) => void) => {
      handlers.finished = h
      return Promise.resolve(() => {})
    }),
    onSyncCompareProgress: vi.fn((h: (p: SyncCompareProgress) => void) => {
      handlers.progress = h
      return Promise.resolve(() => {})
    }),
    onSyncRowsChanged: vi.fn((h: (p: SyncRowsChanged) => void) => {
      handlers.rows = h
      return Promise.resolve(() => {})
    }),
    getSyncConfirm: vi.fn(() => Promise.resolve(preview())),
    startSyncRun: vi.fn(() => Promise.resolve()),
    setSyncRunPaused: vi.fn(() => Promise.resolve()),
    cancelSyncRun: vi.fn(() => Promise.resolve()),
    onSyncRunProgress: listen<SyncRunProgress>((h) => (handlers.runProgress = h)),
    onSyncRunFinished: listen<SyncRunFinished>((h) => (handlers.runFinished = h)),
    getSyncProfiles: vi.fn(() => Promise.resolve([...profiles])),
    saveSyncProfile: vi.fn((p: SyncProfile) => Promise.resolve([...profiles, { type: 'valid' as const, profile: p }])),
    deleteSyncProfile: vi.fn(() => Promise.resolve([...profiles])),
    renameSyncProfile: vi.fn(() => Promise.resolve([...profiles])),
    moveSyncProfile: vi.fn(() => Promise.resolve([...profiles])),
  }
  return {
    ipc,
    profiles,
    runProgress: (progress) => handlers.runProgress?.({ sessionId: 's1', progress }),
    runFinished: (report) => handlers.runFinished?.({ sessionId: 's1', report }),
    finish: (outcome = { type: 'done' }) => handlers.finished?.({ sessionId: 's1', generation, outcome }),
    progress: (payload) => handlers.progress?.({ sessionId: 's1', generation, ...payload }),
  }
}

export const ROOTS = {
  left: { root: { volumeId: 'root', path: '/Users/me/Photos' }, selection: null },
  right: { root: { volumeId: 'root', path: '/Volumes/Backup/Photos' }, selection: null },
}

export async function settle(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve()
}

/** A confirm preview with nothing in it but what `patch` adds. */
export function preview(patch: Partial<ConfirmPreview> = {}): ConfirmPreview {
  return {
    revision: 7,
    groups: [],
    olderOverwritesNewer: [],
    olderOverwritesNewerTotal: 0,
    blocks: [],
    skew: null,
    hiddenActions: 0,
    chosenUnknown: 0,
    keepIsSlow: false,
    ...patch,
  }
}

/** A finished run's report: one row done, nothing else, unless `patch` says so. */
export function report(patch: Partial<RunReportWire> = {}): RunReportWire {
  return {
    done: 1,
    changedSinceCompare: 0,
    noTrash: 0,
    couldNotDo: 0,
    keptNotReplaced: 0,
    held: 0,
    notStarted: 0,
    versionsFolders: [],
    keptBytes: 0,
    operationIds: [],
    items: [{ item: 0, result: 'done' }],
    stopped: false,
    ...patch,
  }
}

/** A saved profile over two local folders. */
export function profile(id: string, name: string): SyncProfile {
  return {
    id,
    name,
    left: { volumeId: 'root', path: `/Users/me/${name}` },
    right: { volumeId: 'backup', path: `/Volumes/Backup/${name}` },
    options: { ...defaultSyncOptions(), mode: 'mirror' },
    showFilters: { ...DEFAULT_SHOW_FILTERS, equal: true },
    restoreShowFilters: false,
    overrides: { ...NO_OVERRIDES },
  }
}
