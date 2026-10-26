/**
 * Store patches for the AI suggestions dialog (`suggestedOpsState`).
 *
 * Each state is a BUILDER, not a literal: a patch carries live `SvelteMap` / `SvelteSet`
 * reviews the dialog mutates as the reviewer ticks boxes and shows more rows, so every opening
 * starts from a fresh copy. The reviews arrive with their windows and live facts already held,
 * so every row state renders without a backend, and nothing here is ever re-read
 * (`ensureLiveFacts` skips ops it already holds).
 *
 * Raw copy on purpose: dev-only, outside the i18n-enforced areas.
 */

import type { SuggestedGroupView, SuggestedOpView, SuggestedSweepView } from '#lib/tauri-commands/index.js'
import {
  newGroupReview,
  suggestedOpsState,
  type GroupReview,
  type LiveFact,
} from '#lib/suggested-ops/suggested-ops-trigger.svelte.js'
import { SvelteMap } from 'svelte/reactivity'
import { daysAgo, hoursAgo } from './relative-time'

type Patch = Partial<typeof suggestedOpsState>

const DOWNLOADS = '/Users/sam/Downloads'

/** One fixture file: its name, and what the disk says about it now (`present` by default). */
interface FixtureFile {
  name: string
  folder?: string
  size: number | null
  /** Unix seconds when suggested. */
  modified: number | null
  newName?: string
  /** What the live check found. Omitted: present and unchanged. */
  live?: LiveFact
}

interface FixtureGroup {
  groupId: number
  verb: SuggestedGroupView['verb']
  displayName: string
  rationale?: string
  sourceFolder?: string | null
  destination?: string | null
  destinationVolumeName?: string | null
  reversible?: SuggestedGroupView['reversible']
  runBlocker?: SuggestedGroupView['runBlocker']
  destinationState?: SuggestedGroupView['destinationState']
  fromSelector?: boolean
  files: FixtureFile[]
  /** The group's real size when it's bigger than the files listed (the 60,000-op case). */
  total?: number
}

function view(group: FixtureGroup, sweepId: number): SuggestedGroupView {
  const total = group.total ?? group.files.length
  return {
    groupId: group.groupId,
    sweepId,
    verb: group.verb,
    status: 'pending',
    displayName: group.displayName,
    rationale: group.rationale ?? null,
    sourceVolumeId: 'root',
    sourceFolder: group.sourceFolder === undefined ? DOWNLOADS : group.sourceFolder,
    destination: group.destination ?? null,
    destinationVolumeName: group.destinationVolumeName ?? null,
    reversible: group.reversible ?? 'restoreMove',
    runBlocker: group.runBlocker ?? null,
    destinationState: group.destinationState ?? (group.destination ? 'exists' : 'notApplicable'),
    liveOpCount: total,
    totalOpCount: total,
    fromSelector: group.fromSelector ?? false,
  }
}

function review(group: FixtureGroup): GroupReview {
  const fresh = newGroupReview()
  const ops: SuggestedOpView[] = group.files.map((file, index) => ({
    opId: index + 1,
    sourcePath: `${file.folder ?? group.sourceFolder ?? DOWNLOADS}/${file.name}`,
    newName: file.newName ?? null,
    status: 'pending',
    snapshotSize: file.size,
    snapshotModified: file.modified,
  }))
  fresh.window = { offset: 0, ops, total: group.total ?? ops.length }
  group.files.forEach((file, index) => {
    const live: LiveFact = file.live ?? { kind: 'present', size: file.size, modified: file.modified ?? hoursAgo(1) }
    fresh.liveFacts.set(index + 1, live)
  })
  return fresh
}

function patch(
  sweeps: Array<{ sweepId: number; createdAt: number; rationale?: string; groups: FixtureGroup[] }>,
): Patch {
  const reviews = new SvelteMap<number, GroupReview>()
  const views: SuggestedSweepView[] = sweeps.map((sweep) => ({
    sweepId: sweep.sweepId,
    createdAt: sweep.createdAt,
    rationale: sweep.rationale ?? null,
    groups: sweep.groups.map((group) => {
      reviews.set(group.groupId, review(group))
      return view(group, sweep.sweepId)
    }),
  }))
  return { open: true, loading: false, loadError: false, sweeps: views, reviews }
}

const MB = 1_000_000

/** The QA case the redesign came from: a month of Downloads, cleaned up to a NAS and the Trash. */
const DOWNLOADS_CLEANUP: FixtureGroup[] = [
  {
    groupId: 101,
    verb: 'move',
    displayName: 'Installers you’ve already run',
    rationale: 'These disk images installed apps that are now in Applications, so you don’t need them here.',
    destination: '/backups/Installers',
    destinationVolumeName: 'Naspolya',
    files: [
      { name: 'Docker.dmg', size: 612 * MB, modified: daysAgo(12) },
      { name: 'Figma-124.5.dmg', size: 291 * MB, modified: daysAgo(20), live: { kind: 'missing' } },
      {
        name: 'Obsidian-1.9.12-universal.dmg',
        size: 181 * MB,
        modified: daysAgo(25),
        live: { kind: 'present', size: 183 * MB, modified: hoursAgo(3) },
      },
      { name: 'Zoom.pkg', size: 74 * MB, modified: daysAgo(28), live: { kind: 'unknown' } },
    ],
  },
  {
    groupId: 102,
    verb: 'move',
    displayName: 'Invoices from 2026',
    rationale: 'They all look like invoices, and you keep the others in your finances folder.',
    destination: '/papers/finances/2026/invoices',
    destinationVolumeName: 'Naspolya',
    destinationState: 'willBeCreated',
    files: [
      { name: 'invoice-2026-07.pdf', size: 184_320, modified: daysAgo(80) },
      { name: 'invoice-2026-08.pdf', size: 190_112, modified: daysAgo(52) },
      { name: 'invoice-2026-09.pdf', size: 176_004, modified: daysAgo(22) },
      { name: 'Számla_Rymdskottkärra_AB_2026_10_végleges.pdf', size: 233_871, modified: daysAgo(3) },
      // Nothing recorded when suggested, so a live value has nothing to have changed from.
      {
        name: 'receipt-ica.pdf',
        size: null,
        modified: null,
        live: { kind: 'present', size: 41_200, modified: daysAgo(2) },
      },
    ],
  },
  {
    groupId: 103,
    verb: 'move',
    displayName: 'Screenshot*.png',
    rationale: 'Screenshots pile up here. Your Pictures folder already has a Screenshots folder.',
    destination: '/photos/screenshots',
    destinationVolumeName: 'Naspolya',
    destinationState: 'unknown',
    fromSelector: true,
    files: Array.from({ length: 14 }, (_, i) => ({
      name: `Screenshot 2026-10-${String(10 + i)} at 11.0${String(i % 10)}.18.png`,
      size: 300_000 + i * 21_000,
      modified: daysAgo(16 - i),
    })),
  },
  {
    groupId: 104,
    verb: 'trash',
    displayName: 'Duplicate downloads',
    rationale: 'Each of these has a copy without the “(1)” in the same folder.',
    files: [
      { name: 'invoice-2026-08 (1).pdf', size: 190_112, modified: daysAgo(52) },
      { name: 'Docker (1).dmg', size: 612 * MB, modified: daysAgo(12) },
      { name: 'IMG_4417 (1).jpeg', size: 3_400_000, modified: daysAgo(6) },
    ],
  },
  {
    groupId: 105,
    verb: 'trash',
    displayName: 'Old exports on the NAS',
    rationale: 'Nobody opened these in over a year.',
    sourceFolder: '/exports/2024',
    runBlocker: 'noTrash',
    files: [
      { name: 'board-deck-2024-q1.key', size: 48 * MB, modified: daysAgo(560) },
      { name: 'board-deck-2024-q2.key', size: 51 * MB, modified: daysAgo(470) },
    ],
  },
  {
    groupId: 106,
    verb: 'rename',
    displayName: 'Scans with dates in their names',
    rationale: 'The scanner names files by number. The dates come from what each page says.',
    destination: DOWNLOADS,
    files: [
      { name: 'scan0007.pdf', newName: 'Scan 2026-05-02.pdf', size: 1_200_000, modified: daysAgo(40) },
      { name: 'scan0008.pdf', newName: 'Scan 2026-05-09.pdf', size: 980_000, modified: daysAgo(33) },
    ],
  },
  {
    groupId: 107,
    verb: 'copy',
    displayName: 'Tax papers to Dropbox',
    rationale: 'You asked to keep a copy of anything tax-related in Dropbox.',
    sourceFolder: null,
    destination: '/Users/sam/Dropbox/Taxes/2026',
    destinationVolumeName: 'Macintosh HD',
    reversible: 'deleteWhatWasWritten',
    files: [
      { name: 'K4-2026.pdf', folder: DOWNLOADS, size: 88_000, modified: daysAgo(9) },
      { name: 'tax-summary.xlsx', folder: '/Users/sam/Documents/Money', size: 42_310, modified: daysAgo(30) },
    ],
  },
]

/** The ones that can't be taken back, and a group of 60,000 that must still open instantly. */
const HARD_CASES: FixtureGroup[] = [
  {
    groupId: 201,
    verb: 'delete',
    displayName: 'Build caches',
    rationale: 'These are regenerated on the next build.',
    sourceFolder: '/Users/sam/projects/cmdr/target',
    reversible: 'irreversible',
    files: [
      { name: 'debug', size: null, modified: daysAgo(1) },
      { name: 'release', size: null, modified: daysAgo(3) },
    ],
  },
  {
    groupId: 202,
    verb: 'compress',
    displayName: 'Archive last year’s photos',
    rationale: 'You don’t open these any more, and they take a lot of space.',
    sourceFolder: '/Users/sam/Pictures/2025',
    destination: '/Users/sam/Pictures/2025.zip',
    destinationVolumeName: 'Macintosh HD',
    reversible: 'irreversible',
    files: [{ name: 'Summer', size: null, modified: daysAgo(300) }],
  },
  {
    groupId: 203,
    verb: 'move',
    displayName: 'Raw camera files',
    rationale: 'Your camera folder has 60,000 raw files you already imported.',
    sourceFolder: '/Volumes/SD/DCIM',
    destination: '/media/raw',
    destinationVolumeName: 'Naspolya',
    total: 60_000,
    files: Array.from({ length: 200 }, (_, i) => ({
      name: `DSC${String(10_000 + i).padStart(5, '0')}.ARW`,
      size: 24 * MB + i * 1_000,
      modified: daysAgo(200 - i / 2),
    })),
  },
]

export const suggestedOpsFixtures: Record<string, () => Patch> = {
  'downloads-cleanup': () =>
    patch([
      {
        sweepId: 1,
        createdAt: hoursAgo(2),
        rationale:
          'Your Downloads folder has 30 files from the last month. Installers and paperwork can go to the NAS, and the duplicates can go.',
        groups: DOWNLOADS_CLEANUP,
      },
    ]),
  'hard-cases': () => patch([{ sweepId: 2, createdAt: daysAgo(3), groups: HARD_CASES }]),
  'changed-and-refused': () => {
    const seeded = patch([{ sweepId: 1, createdAt: hoursAgo(2), groups: DOWNLOADS_CLEANUP.slice(0, 2) }])
    const changed = seeded.reviews?.get(101)
    if (changed) changed.changedUnderReview = true
    return {
      ...seeded,
      decisionNotice: 'suggestedOps.approvalUnsure',
      refusals: new SvelteMap([[102, { kind: 'nothingToRun' }]]),
    }
  },
  empty: () => ({ open: true, loading: false, loadError: false, sweeps: [], reviews: new SvelteMap() }),
  'load-error': () => ({ open: true, loading: false, loadError: true, sweeps: [], reviews: new SvelteMap() }),
}
