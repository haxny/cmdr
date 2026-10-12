import { describe, it, expect, vi } from 'vitest'
import type { TransferTreeRow } from '#lib/tauri-commands/index.js'
import { accessibleName, barFraction, countSegments, rowPaths } from './row-format'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'decimal',
}))

const GB = 1_000_000_000

function folder(overrides: Partial<TransferTreeRow> = {}): TransferTreeRow {
  return {
    id: 7,
    depth: 0,
    label: 'Photos 2024',
    kind: 'folder',
    state: 'active',
    expanded: true,
    hasChildren: true,
    posInSet: 1,
    setSize: 3,
    filesDone: 312,
    filesTotal: 1_000,
    bytesDone: 1.2 * GB,
    bytesTotal: 4 * GB,
    skipped: 0,
    failed: 0,
    failure: null,
    ...overrides,
  }
}

function file(overrides: Partial<TransferTreeRow> = {}): TransferTreeRow {
  return {
    ...folder(),
    id: 9,
    label: 'IMG_0001.jpg',
    kind: 'file',
    expanded: false,
    hasChildren: false,
    filesDone: 0,
    filesTotal: 1,
    bytesDone: 1_000_000,
    bytesTotal: 4_000_000,
    ...overrides,
  }
}

/** The segments as text, an error-toned one marked with a leading `!`. */
function text(row: TransferTreeRow): string[] {
  return countSegments(row, 'copy').map((segment) => (segment.tone === 'error' ? `!${segment.text}` : segment.text))
}

describe('barFraction', () => {
  it('fills a folder along the operation’s bottleneck axis, and a file by bytes alone', () => {
    expect(barFraction(folder(), 'bytes')).toBeCloseTo(0.3)
    expect(barFraction(folder(), 'files')).toBeCloseTo(0.312)
    expect(barFraction(file(), 'files')).toBeCloseTo(0.25)
  })

  it('reads an empty total as done once the row settled, and as nothing before', () => {
    expect(barFraction(file({ bytesDone: 0, bytesTotal: 0, state: 'done' }), 'bytes')).toBe(1)
    expect(barFraction(file({ bytesDone: 0, bytesTotal: 0, state: 'active' }), 'bytes')).toBe(0)
    expect(barFraction(folder({ filesDone: 0, filesTotal: 0, state: 'waiting' }), 'files')).toBe(0)
  })
})

describe('countSegments', () => {
  it('gives a folder at work its progress on both axes', () => {
    expect(text(folder())).toEqual(['312 / 1,000 files', '1.2 GB / 4.0 GB'])
  })

  it('gives a waiting or finished folder its totals, plus what it skipped', () => {
    expect(text(folder({ state: 'waiting', filesDone: 0, bytesDone: 0 }))).toEqual(['1,000 files', '4.0 GB'])
    expect(text(folder({ state: 'done', filesDone: 1_000, bytesDone: 4 * GB, skipped: 3 }))).toEqual([
      '1,000 files',
      '4.0 GB',
      '3 skipped',
    ])
  })

  it('keeps a failed folder’s progress and says how many failed, in the error tone', () => {
    expect(text(folder({ state: 'failed', filesDone: 999, bytesDone: 4 * GB, failed: 1 }))).toEqual([
      '999 / 1,000 files',
      '4.0 GB / 4.0 GB',
      '!1 failed',
    ])
  })

  it('leaves the size out of a folder whose files hold no bytes', () => {
    expect(text(folder({ state: 'waiting', filesDone: 0, filesTotal: 1, bytesDone: 0, bytesTotal: 0 }))).toEqual([
      '1 file',
    ])
  })

  it('gives a file its bytes, a skip its word, and a failure its reason from the typed pipeline', () => {
    expect(text(file())).toEqual(['1.0 MB / 4.0 MB'])
    expect(text(file({ state: 'waiting', bytesDone: 0 }))).toEqual(['4.0 MB'])
    expect(text(file({ state: 'done', bytesDone: 4_000_000, filesDone: 1 }))).toEqual(['4.0 MB'])
    expect(text(file({ state: 'skipped' }))).toEqual(['Skipped'])
    const [reason] = text(file({ state: 'failed', failure: { type: 'source_not_found', path: '/src/IMG_0001.jpg' } }))
    expect(reason.startsWith('!')).toBe(true)
    expect(reason.length).toBeGreaterThan(1)
  })
})

describe('accessibleName', () => {
  it('says what the row is, what’s happening to it, and how far it got, worded per operation', () => {
    expect(accessibleName(folder(), 'copy')).toBe('Photos 2024, Copying, 312 of 1,000 files, 1.2 GB of 4.0 GB')
    expect(accessibleName(file(), 'move')).toBe('IMG_0001.jpg, Moving, 1.0 MB of 4.0 MB')
    expect(accessibleName(file({ state: 'active' }), 'compress')).toBe('IMG_0001.jpg, Compressing, 1.0 MB of 4.0 MB')
  })

  it('names settled rows by their totals and their outcome', () => {
    expect(accessibleName(folder({ state: 'waiting', filesDone: 0, bytesDone: 0 }), 'copy')).toBe(
      'Photos 2024, Waiting, 1,000 files, 4.0 GB',
    )
    expect(accessibleName(folder({ state: 'done', filesDone: 1_000, bytesDone: 4 * GB, skipped: 2 }), 'copy')).toBe(
      'Photos 2024, Done, 1,000 files, 4.0 GB, 2 skipped',
    )
    expect(accessibleName(file({ state: 'skipped' }), 'copy')).toBe('IMG_0001.jpg, Skipped, 4.0 MB')
  })
})

describe('rowPaths', () => {
  it('names every row by its full relative path, from the ancestors above the window', () => {
    const rows = [
      file({ id: 1, depth: 2, label: 'b.txt' }),
      folder({ id: 2, depth: 1, label: 'raw/2024' }),
      file({ id: 3, depth: 2, label: 'c.txt' }),
      folder({ id: 4, depth: 0, label: 'Other' }),
    ]
    const ancestors = [
      { index: 0, row: folder({ id: 10, depth: 0, label: 'Photos' }) },
      { index: 4, row: folder({ id: 11, depth: 1, label: 'edited' }) },
    ]
    expect(rowPaths(rows, ancestors)).toEqual([
      'Photos/edited/b.txt',
      'Photos/raw/2024',
      'Photos/raw/2024/c.txt',
      'Other',
    ])
  })
})
