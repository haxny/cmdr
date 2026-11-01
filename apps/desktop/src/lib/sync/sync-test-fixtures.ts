/**
 * Test fixtures for the Synchronize folders dialog: a fake IPC whose compare finishes when told, a small view with
 * root rows, a folder header, and a warning row, and a summary with a guard. Test-only.
 */
import { vi } from 'vitest'
import type {
  SyncCompareFinished,
  SyncCompareProgress,
  SyncRowsChanged,
  SyncState,
  ViewRowWire,
} from '#lib/tauri-commands/index.js'
import type { SyncIpc } from './sync-session.svelte'
import type { SyncItemRow } from './row-label'

export function itemRow(index: number, patch: Partial<SyncItemRow> = {}): SyncItemRow {
  return {
    type: 'item',
    index,
    leftName: `file-${String(index)}.txt`,
    rightName: `file-${String(index)}.txt`,
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
    rightKind: null,
    rightName: null,
    rightSize: null,
    rightMtime: null,
    category: 'leftOnly',
    rung: 'presence',
  }),
  itemRow(3, {
    category: 'newerRight',
    olderOverwritesNewer: true,
    defaultAction: 'skip',
    rung: 'content',
    content: 'pending',
  }),
  itemRow(4, {
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
}

export interface FakeSyncIpc {
  ipc: SyncIpc
  finish: (outcome?: SyncCompareFinished['outcome']) => void
  progress: (payload: Omit<SyncCompareProgress, 'sessionId' | 'generation'>) => void
}

export function fakeSyncIpc(view: ViewRowWire[] = VIEW, state: SyncState = STATE): FakeSyncIpc {
  const handlers: {
    finished?: (p: SyncCompareFinished) => void
    progress?: (p: SyncCompareProgress) => void
    rows?: (p: SyncRowsChanged) => void
  } = {}
  let generation = 0
  const ipc: SyncIpc = {
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
    setSyncActions: vi.fn(() => Promise.resolve(state)),
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
  }
  return {
    ipc,
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
