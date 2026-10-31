import { describe, expect, it } from 'vitest'
import { nextRenameSelection, renameSelectionStages, wordStopSelection, type WordStopInput } from './rename-selection'

describe('renameSelectionStages', () => {
  it('cycles name, whole name, extension for a file with one', () => {
    expect(renameSelectionStages('report.pdf', 'file')).toEqual([
      { start: 0, end: 6 },
      { start: 0, end: 10 },
      { start: 7, end: 10 },
    ])
  })

  it('treats a compound tar as one extension', () => {
    expect(renameSelectionStages('src.tar.gz', 'file')).toEqual([
      { start: 0, end: 3 },
      { start: 0, end: 10 },
      { start: 4, end: 10 },
    ])
  })

  it('has one stage for a name with no extension, so the steps that would repeat it are skipped', () => {
    expect(renameSelectionStages('README', 'file')).toEqual([{ start: 0, end: 6 }])
    expect(renameSelectionStages('.gitignore', 'file')).toEqual([{ start: 0, end: 10 }])
    expect(renameSelectionStages('Smlouva v. 2 final', 'file')).toEqual([{ start: 0, end: 18 }])
  })

  it('has one stage for a plain folder, dot or no dot, and three for a package', () => {
    expect(renameSelectionStages('release.v1.2', 'folder')).toEqual([{ start: 0, end: 12 }])
    expect(renameSelectionStages('Cmdr.app', 'package')).toEqual([
      { start: 0, end: 4 },
      { start: 0, end: 8 },
      { start: 5, end: 8 },
    ])
  })

  it('has no stage for an empty field', () => {
    expect(renameSelectionStages('', 'file')).toEqual([])
  })
})

describe('nextRenameSelection', () => {
  it('moves to the next stage and wraps around', () => {
    expect(nextRenameSelection('report.pdf', 'file', { start: 0, end: 6 })).toEqual({ start: 0, end: 10 })
    expect(nextRenameSelection('report.pdf', 'file', { start: 0, end: 10 })).toEqual({ start: 7, end: 10 })
    expect(nextRenameSelection('report.pdf', 'file', { start: 7, end: 10 })).toEqual({ start: 0, end: 6 })
  })

  it('starts over at the name when the selection is anything else', () => {
    expect(nextRenameSelection('report.pdf', 'file', { start: 3, end: 3 })).toEqual({ start: 0, end: 6 })
  })

  it('reads the name as typed, not as it was', () => {
    expect(nextRenameSelection('report.docx', 'file', { start: 0, end: 6 })).toEqual({ start: 0, end: 11 })
  })

  it('keeps reselecting the whole name when that is the only stage', () => {
    expect(nextRenameSelection('README', 'file', { start: 0, end: 6 })).toEqual({ start: 0, end: 6 })
  })

  it('leaves the caret where it is in an empty field', () => {
    expect(nextRenameSelection('', 'file', { start: 0, end: 0 })).toEqual({ start: 0, end: 0 })
  })
})

function stop(overrides: Partial<WordStopInput>) {
  return wordStopSelection({
    value: 'IMG_2041.jpg',
    nameKind: 'file',
    selectionStart: 12,
    selectionEnd: 12,
    selectionDirection: 'none',
    direction: 'left',
    extend: false,
    ...overrides,
  })
}

describe('wordStopSelection', () => {
  it('stops a word-left at the start of the extension', () => {
    expect(stop({})).toEqual({ start: 9, end: 9, direction: 'none' })
  })

  it('selects exactly the extension with shift from the end', () => {
    expect(stop({ extend: true })).toEqual({ start: 9, end: 12, direction: 'backward' })
  })

  it('stops a word-right at the end of the name', () => {
    expect(stop({ selectionStart: 0, selectionEnd: 0, direction: 'right' })).toEqual({
      start: 8,
      end: 8,
      direction: 'none',
    })
  })

  it('selects exactly the name with shift from the start', () => {
    expect(stop({ selectionStart: 0, selectionEnd: 0, direction: 'right', extend: true })).toEqual({
      start: 0,
      end: 8,
      direction: 'forward',
    })
  })

  it('stops a word-right that would skip the space before the last word', () => {
    expect(stop({ value: 'my report.pdf', selectionStart: 2, selectionEnd: 2, direction: 'right' })).toEqual({
      start: 9,
      end: 9,
      direction: 'none',
    })
  })

  it('leaves the move to the system when a native word boundary comes first', () => {
    // From the start of `my report.pdf`, ⌥→ ends at `my`, well before the dot.
    expect(stop({ value: 'my report.pdf', selectionStart: 0, selectionEnd: 0, direction: 'right' })).toBeNull()
    // From the dot's own stops, the next move is the system's.
    expect(stop({ selectionStart: 9, selectionEnd: 9 })).toBeNull()
    expect(stop({ selectionStart: 8, selectionEnd: 8, direction: 'right' })).toBeNull()
    // Left of the extension, ⌥← is an ordinary word move.
    expect(stop({ selectionStart: 5, selectionEnd: 5 })).toBeNull()
  })

  it('does nothing for a name without an extension', () => {
    expect(stop({ value: 'Smlouva v. 2 final', selectionStart: 18, selectionEnd: 18 })).toBeNull()
    expect(stop({ value: 'release.v1', nameKind: 'folder', selectionStart: 10, selectionEnd: 10 })).toBeNull()
  })

  it('leaves a compound extension to the system, which may stop inside it', () => {
    expect(stop({ value: 'src.tar.gz', selectionStart: 10, selectionEnd: 10 })).toBeNull()
  })

  it('leaves a plain move with a selection to the system', () => {
    expect(stop({ selectionStart: 0, selectionEnd: 8 })).toBeNull()
  })

  it('extends from the moving end of a selection, keeping its anchor', () => {
    // Name selected, as F2 opens it; ⇧⌥→ from its end reaches past the dot natively.
    expect(
      stop({ selectionStart: 0, selectionEnd: 8, selectionDirection: 'forward', direction: 'right', extend: true }),
    ).toBeNull()
    // Caret mid-extension, anchor at the end: ⇧⌥← moves the focus to the extension's start.
    expect(stop({ selectionStart: 10, selectionEnd: 12, selectionDirection: 'backward', extend: true })).toEqual({
      start: 9,
      end: 12,
      direction: 'backward',
    })
  })
})
