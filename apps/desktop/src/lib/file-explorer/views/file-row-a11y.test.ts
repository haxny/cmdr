import { describe, it, expect } from 'vitest'
import { fileRowAttributes, fileRowId, fileRowKind, fileRowName, renderedCursorRow, rowSizeText } from './file-row-a11y'
import { createParentEntry } from './file-list-utils'
import type { FileEntry } from '../types'

function entry(overrides: Partial<FileEntry> & { name: string }): FileEntry {
  return {
    path: `/root/${overrides.name}`,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: '',
    group: '',
    iconId: 'file',
    extendedMetadataLoaded: true,
    ...overrides,
  }
}

describe('fileRowKind', () => {
  it('names folders, symlinks, packages, and archives, and gives a plain file no kind', () => {
    expect(fileRowKind(entry({ name: 'sub', isDirectory: true }))).toBe('folder')
    expect(fileRowKind(entry({ name: 'link', isSymlink: true }))).toBe('symlink')
    expect(fileRowKind(entry({ name: 'Safari.app', isDirectory: true, isPackage: true }))).toBe('package')
    expect(fileRowKind(entry({ name: 'c.zip', isArchive: true }))).toBe('archive')
    expect(fileRowKind(entry({ name: 'a.txt' }))).toBeUndefined()
  })

  it('calls a symlink to a folder a symlink, the way its badge does', () => {
    expect(fileRowKind(entry({ name: 'docs-link', isDirectory: true, isSymlink: true }))).toBe('symlink')
  })
})

describe('fileRowName', () => {
  it('says the name, then the kind', () => {
    expect(fileRowName({ entry: entry({ name: 'subfolder', isDirectory: true }) })).toBe('subfolder, folder')
    expect(fileRowName({ entry: entry({ name: 'Safari.app', isDirectory: true, isPackage: true }) })).toBe(
      'Safari.app, package',
    )
    expect(fileRowName({ entry: entry({ name: 'c.zip', isArchive: true }) })).toBe('c.zip, archive')
    expect(fileRowName({ entry: entry({ name: 'd-link', isSymlink: true }) })).toBe('d-link, symlink')
  })

  it('gives a plain file its name alone, since the extension is in it', () => {
    expect(fileRowName({ entry: entry({ name: 'a.txt' }) })).toBe('a.txt')
  })

  it('names the `..` row "Parent folder" and nothing else, details included', () => {
    const parent = createParentEntry('/')
    expect(fileRowName({ entry: parent })).toBe('Parent folder')
    expect(fileRowName({ entry: parent, details: { size: '4 KB', modified: '2026-10-14 10:30' } })).toBe(
      'Parent folder',
    )
  })

  it('says what the row’s badges mean after the kind', () => {
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), syncStatus: 'online_only' })).toBe('a.txt, Online-only')
    expect(fileRowName({ entry: entry({ name: 'Desktop', isDirectory: true }), isRestricted: true })).toBe(
      'Desktop, folder, Restricted folder',
    )
    expect(fileRowName({ entry: entry({ name: 'cold.bin', inColdStorage: true }) })).toBe('cold.bin, Archived')
    // `unknown` draws no badge, so it says nothing either.
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), syncStatus: 'unknown' })).toBe('a.txt')
  })

  it('adds the Full view’s size, modified date, and git status in that order', () => {
    const details = { size: '1 KB', modified: '2026-10-14 10:30', gitStatus: 'Modified' }
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), details })).toBe(
      'a.txt, 1 KB, modified 2026-10-14 10:30, Modified',
    )
    expect(fileRowName({ entry: entry({ name: 'sub', isDirectory: true }), details })).toBe(
      'sub, folder, 1 KB, modified 2026-10-14 10:30, Modified',
    )
  })

  it('leaves out a size, date, or git status it doesn’t have', () => {
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), details: { modified: '2026-10-14 10:30' } })).toBe(
      'a.txt, modified 2026-10-14 10:30',
    )
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), details: { size: '', modified: '' } })).toBe('a.txt')
  })
})

describe('rowSizeText', () => {
  const triads = [
    { value: '1', tierClass: 'size-kb' },
    { value: ' 024', tierClass: 'size-b' },
  ]

  it('says the size the cell shows, joined', () => {
    expect(rowSizeText({ triads })).toBe('1 024')
    expect(rowSizeText({ triads, dirState: 'size' })).toBe('1 024')
    expect(rowSizeText({ triads, dirState: 'lower-bound' })).toBe('≥1 024')
  })

  it('says the cell’s override word when it shows one', () => {
    expect(rowSizeText({ override: 'No access', triads })).toBe('No access')
  })

  it('says no size while the cell shows the `<dir>` placeholder or nothing', () => {
    expect(rowSizeText({ triads, dirState: 'dir' })).toBeUndefined()
    expect(rowSizeText({ triads, dirState: 'scanning' })).toBeUndefined()
    expect(rowSizeText({})).toBeUndefined()
  })
})

describe('fileRowAttributes', () => {
  it('gives every option its id, name, marked state, and place in the whole list', () => {
    expect(fileRowAttributes({ listId: 's1', globalIndex: 2, totalCount: 8, isMarked: true, name: 'a.txt' })).toEqual({
      id: 's1-row-2',
      'aria-label': 'a.txt',
      'aria-selected': true,
      'aria-posinset': 3,
      'aria-setsize': 8,
    })
  })

  // ❗ VoiceOver invents a position from the RENDERED siblings for an option without one
  // ("Parent folder (1 of 8)" beside "subfolder (1 of 7)"), so the `..` row counts too.
  it('counts the `..` row as position 1, and marks an unmarked row `aria-selected="false"`', () => {
    expect(
      fileRowAttributes({ listId: 's1', globalIndex: 0, totalCount: 8, isMarked: false, name: 'Parent folder' }),
    ).toMatchObject({ 'aria-selected': false, 'aria-posinset': 1, 'aria-setsize': 8 })
  })
})

describe('renderedCursorRow', () => {
  const rows = [
    { file: entry({ name: 'a.txt' }), globalIndex: 4 },
    { file: entry({ name: 'b.txt' }), globalIndex: 5 },
  ]

  it('names the cursor row by its id and path when it’s rendered', () => {
    expect(renderedCursorRow('s1', 5, rows)).toEqual({ id: fileRowId('s1', 5), key: '/root/b.txt' })
  })

  // A dangling `aria-activedescendant` is an axe critical (`aria-valid-attr-value`) and
  // leaves VoiceOver reading a stale row.
  it('names nothing when the cursor row is outside the virtual window', () => {
    expect(renderedCursorRow('s1', 9, rows)).toBeUndefined()
    expect(renderedCursorRow('s1', 0, [])).toBeUndefined()
  })
})
