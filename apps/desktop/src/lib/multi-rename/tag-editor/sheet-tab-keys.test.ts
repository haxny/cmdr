import { describe, it, expect } from 'vitest'
import { sheetTabOfKey } from './sheet-tab-keys'

const base = { key: '1', metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, isComposing: false }

describe('sheetTabOfKey', () => {
  it('maps ⌘1 to Rename and ⌘2 to Tags', () => {
    expect(sheetTabOfKey(base)).toBe('rename')
    expect(sheetTabOfKey({ ...base, key: '2' })).toBe('tags')
  })

  it('ignores other keys and extra modifiers', () => {
    expect(sheetTabOfKey({ ...base, key: '3' })).toBeNull()
    expect(sheetTabOfKey({ ...base, metaKey: false })).toBeNull()
    expect(sheetTabOfKey({ ...base, altKey: true })).toBeNull()
    expect(sheetTabOfKey({ ...base, shiftKey: true })).toBeNull()
    expect(sheetTabOfKey({ ...base, isComposing: true })).toBeNull()
  })
})
