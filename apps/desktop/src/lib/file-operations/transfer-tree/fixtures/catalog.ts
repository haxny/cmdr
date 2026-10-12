/**
 * The six transfer shapes the tree must look right on, as scripted transfers:
 * the Debug catalog plays them live, and the component and a11y tests step
 * them. Names and sizes are made up (and deterministic).
 */

import type { TransferTreeOperation } from '../row-format'
import { createScriptedTransfer, type FixtureNode, type ScriptedTransfer } from './scripted-transfer'

const KB = 1_000
const MB = 1_000_000
const GB = 1_000_000_000

export interface TransferTreeFixture {
  id: string
  /** A Debug-catalog heading (dev only, not translated). */
  title: string
  operation: TransferTreeOperation
  create: () => ScriptedTransfer
}

/** Deterministic sizes in `[min, max)`, so every run of a fixture looks the same. */
function sizes(seed: number, min: number, max: number): () => number {
  let state = seed
  return () => {
    state = (state * 1_103_515_245 + 12_345) % 2_147_483_648
    return Math.round(min + (state / 2_147_483_648) * (max - min))
  }
}

function files(count: number, name: (i: number) => string, size: () => number): FixtureNode[] {
  return Array.from({ length: count }, (_, i) => ({ name: name(i + 1), size: size() }))
}

const pad = (n: number, width: number): string => String(n).padStart(width, '0')

export const TRANSFER_TREE_FIXTURES: readonly TransferTreeFixture[] = [
  {
    id: 'single-file',
    title: 'Single file',
    operation: 'copy',
    create: () =>
      createScriptedTransfer({
        top: [{ name: 'Holiday 2025.mov', size: 3.2 * GB }],
        bytesPerStep: 40 * MB,
        preparingSteps: 5,
      }),
  },
  {
    id: 'flat-5000',
    title: '5,000 top-level files',
    operation: 'copy',
    create: () =>
      createScriptedTransfer({
        top: files(5_000, (i) => `IMG_${pad(i, 4)}.jpg`, sizes(7, 1 * MB, 8 * MB)),
        parallel: 2,
        bytesPerStep: 4 * MB,
        preparingSteps: 5,
        axis: 'files',
      }),
  },
  {
    id: 'folders-5x1000',
    title: '5 folders × 1,000 files',
    operation: 'move',
    create: () =>
      createScriptedTransfer({
        top: Array.from({ length: 5 }, (_, folder) => ({
          name: `Album ${String(folder + 1)}`,
          children: files(1_000, (i) => `track_${pad(i, 4)}.flac`, sizes(folder + 11, 20 * MB, 45 * MB)),
        })),
        bytesPerStep: 30 * MB,
        preparingSteps: 5,
      }),
  },
  {
    id: 'deep-compact',
    title: 'Deep a/b/c/1, compact folders',
    operation: 'compress',
    create: () =>
      createScriptedTransfer({
        top: [
          {
            name: 'a',
            children: [
              { name: 'b', children: [{ name: 'c', children: files(12, String, sizes(3, 40 * KB, 900 * KB)) }] },
            ],
          },
          {
            name: 'Photos',
            children: [
              {
                name: '2024',
                children: [
                  { name: 'Summer', children: files(8, (i) => `IMG_${pad(i, 4)}.heic`, sizes(5, 2 * MB, 4 * MB)) },
                  {
                    name: 'Winter',
                    children: files(8, (i) => `IMG_${pad(i + 100, 4)}.heic`, sizes(6, 2 * MB, 4 * MB)),
                  },
                ],
              },
            ],
          },
          { name: 'notes.txt', size: 12 * KB },
        ],
        bytesPerStep: 300 * KB,
        preparingSteps: 5,
      }),
  },
  {
    id: 'parallel-smb',
    title: 'Parallel SMB, 8 files in flight across 3 folders',
    operation: 'copy',
    create: () =>
      createScriptedTransfer({
        top: [
          { name: 'Contracts', children: files(3, (i) => `contract-${String(i)}.pdf`, sizes(21, 80 * MB, 120 * MB)) },
          { name: 'Invoices', children: files(3, (i) => `invoice-${pad(i, 3)}.pdf`, sizes(22, 40 * MB, 70 * MB)) },
          { name: 'Scans', children: files(24, (i) => `scan-${pad(i, 3)}.tiff`, sizes(23, 5 * MB, 40 * MB)) },
        ],
        parallel: 8,
        bytesPerStep: 3 * MB,
        preparingSteps: 5,
      }),
  },
  {
    id: 'failed-and-skipped',
    title: 'Failed and skipped rows',
    operation: 'copy',
    create: () =>
      createScriptedTransfer({
        top: [
          {
            name: 'Reports',
            children: files(10, (i) => `report-${pad(i, 2)}.xlsx`, sizes(31, 1 * MB, 3 * MB)).map((node, i) => ({
              ...node,
              outcome: i === 2 || i === 7 ? 'skip' : i === 5 ? 'fail' : undefined,
            })),
          },
          { name: 'draft.docx', size: 400 * KB, outcome: 'skip' },
          { name: 'locked.pdf', size: 2 * MB, outcome: 'fail' },
          { name: 'summary.pdf', size: 1 * MB },
        ],
        bytesPerStep: 500 * KB,
        preparingSteps: 5,
      }),
  },
]
