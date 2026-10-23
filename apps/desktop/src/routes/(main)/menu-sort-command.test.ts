import { describe, expect, it } from 'vitest'
import { menuSortToCommand } from './menu-sort-command'

describe('menuSortToCommand', () => {
  it('maps every View > Sort by column to its focused-pane command', () => {
    expect(menuSortToCommand('sortBy', 'name')).toBe('sort.byName')
    expect(menuSortToCommand('sortBy', 'extension')).toBe('sort.byExtension')
    expect(menuSortToCommand('sortBy', 'size')).toBe('sort.bySize')
    expect(menuSortToCommand('sortBy', 'modified')).toBe('sort.byModified')
    expect(menuSortToCommand('sortBy', 'created')).toBe('sort.byCreated')
    expect(menuSortToCommand('sortBy', 'added')).toBe('sort.byAdded')
  })

  it('maps the order items', () => {
    expect(menuSortToCommand('sortOrder', 'asc')).toBe('sort.ascending')
    expect(menuSortToCommand('sortOrder', 'desc')).toBe('sort.descending')
    expect(menuSortToCommand('sortOrder', 'toggle')).toBe('sort.toggleOrder')
  })

  it('answers undefined for a payload it does not know', () => {
    expect(menuSortToCommand('sortBy', 'date')).toBeUndefined()
    expect(menuSortToCommand('sortBy', 'toString')).toBeUndefined()
    expect(menuSortToCommand('sortBy', 42)).toBeUndefined()
    expect(menuSortToCommand('sortOrder', 'up')).toBeUndefined()
    expect(menuSortToCommand('sortWhat', 'name')).toBeUndefined()
  })
})
