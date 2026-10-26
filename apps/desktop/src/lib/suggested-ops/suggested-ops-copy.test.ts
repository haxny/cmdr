/**
 * The words a group reads out and the facts each row compares. The button has to name the action
 * the verb runs, the reversibility pill has to give the RIGHT reason (a trash on a drive with no
 * Trash must never claim one), and a row has to tell a value that moved from one that didn't.
 */

import { describe, expect, it } from 'vitest'
import type { SuggestedOpView } from '#lib/tauri-commands/index.js'
import {
  approveKey,
  fileIconId,
  fileName,
  parentFolder,
  reversibilityKey,
  rowFacts,
  runBlockerKeys,
} from './suggested-ops-copy'

function op(overrides: Partial<SuggestedOpView> = {}): SuggestedOpView {
  return {
    opId: 1,
    sourcePath: '/Users/someone/Downloads/invoice.pdf',
    newName: null,
    status: 'pending',
    snapshotSize: 1_000,
    snapshotModified: 1_780_000_000,
    ...overrides,
  }
}

describe('the approve button', () => {
  it('names the action each verb runs', () => {
    expect(approveKey('move')).toBe('suggestedOps.approve.move')
    expect(approveKey('trash')).toBe('suggestedOps.approve.trash')
    expect(approveKey('delete')).toBe('suggestedOps.approve.delete')
    expect(approveKey('extract')).toBe('suggestedOps.approve.extract')
  })
})

describe('the reversibility pill', () => {
  it('says a trash goes to the Trash, and a move can be moved back', () => {
    expect(reversibilityKey('trash', 'restoreMove')).toBe('suggestedOps.reversible.trash')
    expect(reversibilityKey('move', 'restoreMove')).toBe('suggestedOps.reversible.moveBack')
    expect(reversibilityKey('rename', 'restoreMove')).toBe('suggestedOps.reversible.renameBack')
  })

  it('says what undoing a write removes', () => {
    expect(reversibilityKey('copy', 'deleteWhatWasWritten')).toBe('suggestedOps.reversible.removeCopies')
    expect(reversibilityKey('compress', 'deleteWhatWasWritten')).toBe('suggestedOps.reversible.removeArchive')
    expect(reversibilityKey('extract', 'deleteWhatWasWritten')).toBe('suggestedOps.reversible.removeExtracted')
  })

  it('says a trash on a drive with no Trash can’t run, rather than promising an undo', () => {
    expect(runBlockerKeys('noTrash').pill).toBe('suggestedOps.blocked.noTrash')
  })

  it('says why a delete or an overwrite is for good', () => {
    expect(reversibilityKey('delete', 'irreversible')).toBe('suggestedOps.irreversible.delete')
    expect(reversibilityKey('compress', 'irreversible')).toBe('suggestedOps.irreversible.overwrite')
    expect(reversibilityKey('move', 'irreversible')).toBe('suggestedOps.irreversible.other')
  })
})

describe('a row’s facts', () => {
  it('shows the live value and flags only what moved since the suggestion', () => {
    const facts = rowFacts(op(), { kind: 'present', size: 2_000, modified: 1_780_000_000 })

    expect(facts).toMatchObject({ kind: 'live', size: 2_000, sizeChanged: true, modifiedChanged: false })
  })

  it('flags nothing when the suggestion recorded nothing to compare against', () => {
    const facts = rowFacts(op({ snapshotSize: null, snapshotModified: null }), {
      kind: 'present',
      size: 2_000,
      modified: 1_790_000_000,
    })

    expect(facts).toMatchObject({ kind: 'live', sizeChanged: false, modifiedChanged: false })
  })

  it('says a file is gone', () => {
    expect(rowFacts(op(), { kind: 'missing' })).toEqual({ kind: 'missing' })
  })

  it('falls back to the snapshot when Cmdr couldn’t look, or hasn’t yet', () => {
    expect(rowFacts(op(), { kind: 'unknown' })).toEqual({ kind: 'snapshot', size: 1_000, modified: 1_780_000_000 })
    expect(rowFacts(op(), undefined)).toEqual({ kind: 'snapshot', size: 1_000, modified: 1_780_000_000 })
  })
})

describe('paths', () => {
  it('splits a path into the name and its folder', () => {
    expect(fileName('/Users/someone/Downloads/invoice.pdf')).toBe('invoice.pdf')
    expect(parentFolder('/Users/someone/Downloads/invoice.pdf')).toBe('/Users/someone/Downloads')
    expect(parentFolder('/invoice.pdf')).toBe('/')
    expect(fileName('/Users/someone/Downloads/Old stuff/')).toBe('Old stuff')
  })

  it('keys the icon the way the backend does', () => {
    expect(fileIconId('/a/Report.PDF')).toBe('ext:pdf')
    expect(fileIconId('/a/Makefile')).toBe('file')
    expect(fileIconId('/a/.zshrc')).toBe('file')
  })
})
