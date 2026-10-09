/**
 * The presets menu (F2): what it offers to load, and that Save and Delete
 * appear only for a loaded saved preset.
 */

import { describe, it, expect } from 'vitest'
import { presetMenuSections } from './preset-menu'

const mine = { id: 'p1', name: 'Fotky', spec: { nameMask: '[N]', extensionMask: '[E]' } } as never

describe('presetMenuSections', () => {
  it('lists Default and the built-ins, then the saved presets, and marks the loaded one', () => {
    const [builtIn, saved, manage] = presetMenuSections({ saved: [mine], selectedId: 'p1', loaded: mine })
    expect(builtIn.items.map((i) => i.data)).toEqual([
      { kind: 'load', id: 'default' },
      { kind: 'load', id: 'builtin:remove-diacritics' },
      { kind: 'load', id: 'builtin:greek-to-latin' },
      { kind: 'load', id: 'builtin:normalize-unicode' },
    ])
    expect(saved.items.map((i) => [i.label, i.check])).toEqual([['Fotky', { kind: 'current' }]])
    expect(manage.items.map((i) => i.data?.kind)).toEqual(['saveAs', 'save', 'delete'])
  })

  it('offers only Save as while no saved preset is loaded', () => {
    const [, saved, manage] = presetMenuSections({ saved: [], selectedId: 'builtin:greek-to-latin', loaded: null })
    expect(saved.items).toEqual([])
    expect(saved.emptyLabel).toBeTruthy()
    expect(manage.items.map((i) => i.data?.kind)).toEqual(['saveAs'])
  })
})
