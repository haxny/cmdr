import { describe, expect, it } from 'vitest'
import { showInGridFilters, versionsFolderPane } from './sync-run-view'
import { ROOTS } from './sync-test-fixtures'

describe('sync run view helpers', () => {
  it('shows a versions folder in the pane on its volume, the right one when both or neither are', () => {
    const roots = { ...ROOTS, left: { ...ROOTS.left, root: { volumeId: 'local', path: '/a' } } }
    expect(versionsFolderPane(roots, 'local')).toBe('left')
    expect(versionsFolderPane(roots, 'root')).toBe('right')
    expect(versionsFolderPane(roots, 'elsewhere')).toBe('right')
  })

  it('Show in grid shows every row, equal ones too', () => {
    expect(Object.entries(showInGridFilters()).every(([key, on]) => (key === 'warningsOnly' ? !on : on))).toBe(true)
  })
})
