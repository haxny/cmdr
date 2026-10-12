import { describe, it, expect } from 'vitest'
import { treeOperationFor } from './tree-operation'

describe('treeOperationFor', () => {
  it('shows a tree for a copy, move, or compress once it writes', () => {
    expect(treeOperationFor({ operationType: 'copy', phase: 'copying', reversing: false })).toBe('copy')
    expect(treeOperationFor({ operationType: 'move', phase: 'deleting', reversing: false })).toBe('move')
    expect(treeOperationFor({ operationType: 'compress', phase: 'compressing', reversing: false })).toBe('compress')
  })

  it('shows none for an operation that never builds one', () => {
    for (const operationType of ['delete', 'trash', 'archive_edit', 'rename', 'create_folder'] as const) {
      expect(treeOperationFor({ operationType, phase: 'copying', reversing: false })).toBeNull()
    }
  })

  it('shows none before the scan is over, or before a view has heard where the operation is', () => {
    expect(treeOperationFor({ operationType: 'copy', phase: 'scanning', reversing: false })).toBeNull()
    expect(treeOperationFor({ operationType: 'copy', phase: null, reversing: false })).toBeNull()
  })

  it('shows none while a reversal runs: its rows would call going files done', () => {
    expect(treeOperationFor({ operationType: 'copy', phase: 'rolling_back', reversing: true })).toBeNull()
    expect(treeOperationFor({ operationType: 'move', phase: 'copying', reversing: true })).toBeNull()
  })
})
