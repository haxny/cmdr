import { describe, it, expect } from 'vitest'
import type { MultiRenameError } from '#lib/tauri-commands/index.js'
import { errorText, patternErrorAt } from './error-text'

const pattern = (error: Extract<MultiRenameError, { type: 'spec' }>['error']): MultiRenameError => ({
  type: 'spec',
  error,
})

describe('the error line', () => {
  it('says what a foobar pattern breaks on and where, counting characters as the user sees them', () => {
    const mask = 'Deník %nope%'
    const error = pattern({ type: 'pattern', error: { type: 'unknownField', name: 'nope', at: 'Deník '.length + 1 } })
    expect(errorText(error, mask)).toBe('Cmdr doesn’t know the field %nope% (character 7)')
    expect(patternErrorAt(error, mask)).toBe(6)
  })

  it('words every pattern error', () => {
    const at = 0
    expect(
      errorText(pattern({ type: 'pattern', error: { type: 'unknownFunction', name: 'foo', at } }), '$foo(x)'),
    ).toBe('Cmdr doesn’t know the function $foo (character 1)')
    expect(
      errorText(pattern({ type: 'pattern', error: { type: 'badArguments', function: 'num', at } }), '$num(1)'),
    ).toContain('$num')
    expect(errorText(pattern({ type: 'pattern', error: { type: 'unclosed', what: 'bracket', at } }), '[x')).toBe(
      'A “[” is missing its closing “]” (character 1)',
    )
    expect(errorText(pattern({ type: 'pattern', error: { type: 'tooDeep', at } }), '[[')).toContain('(character 1)')
  })

  it('has no place for a Cmdr mask error', () => {
    const error = pattern({ type: 'nameMask', error: { type: 'unclosed', at: 1 } })
    expect(errorText(error, '[N')).toBe('A placeholder is missing its closing “]”')
    expect(patternErrorAt(error, '[N')).toBeNull()
    expect(patternErrorAt(null, '[N')).toBeNull()
  })
})
