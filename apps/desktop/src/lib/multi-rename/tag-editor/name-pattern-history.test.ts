import { describe, expect, it } from 'vitest'
import { withPattern } from './name-pattern-history'

describe('withPattern', () => {
  it('puts the newest first', () => {
    expect(withPattern([{ kind: 'format', pattern: 'a' }], { kind: 'format', pattern: 'b' })).toEqual([
      { kind: 'format', pattern: 'b' },
      { kind: 'format', pattern: 'a' },
    ])
  })

  it('moves a repeated pattern up instead of listing it twice, but keeps the same text in the other mode', () => {
    const history = withPattern(
      [
        { kind: 'format', pattern: 'a' },
        { kind: 'regex', pattern: 'a' },
        { kind: 'format', pattern: 'b' },
      ],
      { kind: 'format', pattern: 'b' },
    )
    expect(history).toEqual([
      { kind: 'format', pattern: 'b' },
      { kind: 'format', pattern: 'a' },
      { kind: 'regex', pattern: 'a' },
    ])
  })

  it('keeps the last ten', () => {
    let history: ReturnType<typeof withPattern> = []
    for (let i = 0; i < 12; i++) history = withPattern(history, { kind: 'format', pattern: `p${String(i)}` })
    expect(history).toHaveLength(10)
    expect(history[0]).toEqual({ kind: 'format', pattern: 'p11' })
    expect(history[9]).toEqual({ kind: 'format', pattern: 'p2' })
  })
})
