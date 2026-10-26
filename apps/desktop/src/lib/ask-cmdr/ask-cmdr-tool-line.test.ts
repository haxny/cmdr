/**
 * The shared readings of a tool line's typed facts.
 */

import { describe, expect, it } from 'vitest'
import type { RailToolCall } from './ask-cmdr-messages'
import { outcomeOk, placeName, showsRepeatTag } from './ask-cmdr-tool-line'

describe('outcomeOk', () => {
  /** The backend's `ToolOutcome::ok`: a refusal and a plan that staged nothing aren't ok. */
  it('reads a refusal and an empty plan as not ok, and every answer as ok', () => {
    expect(outcomeOk({ kind: 'refused', reason: { kind: 'toolUnavailable' } })).toBe(false)
    expect(outcomeOk({ kind: 'notStaged' })).toBe(false)
    expect(outcomeOk({ kind: 'nothingFound', lowerBound: true })).toBe(true)
    expect(outcomeOk({ kind: 'indexUnavailable', reason: 'imageIndexingOff' })).toBe(true)
    expect(outcomeOk({ kind: 'staged', count: 3 })).toBe(true)
  })
})

describe('placeName', () => {
  it('shows a path’s last segment, and the root as itself', () => {
    expect(placeName('/Users/me/Downloads')).toBe('Downloads')
    expect(placeName('/Users/me/Downloads/')).toBe('Downloads')
    expect(placeName('/a/notes.txt')).toBe('notes.txt')
    expect(placeName('/')).toBe('/')
    expect(placeName('prefs.md')).toBe('prefs.md')
  })
})

describe('showsRepeatTag', () => {
  const call = (overrides: Partial<RailToolCall>): RailToolCall => ({
    callId: 'c1',
    tool: 'list_volumes',
    running: false,
    subject: { kind: 'drives' },
    outcome: { kind: 'answered' },
    repeatOf: null,
    ...overrides,
  })

  it('tags a repeat, but not the breaker’s refusal, which says so itself', () => {
    const repeatOf = { messageId: 1, callId: 'fake-call-0' }
    expect(showsRepeatTag(call({}))).toBe(false)
    expect(showsRepeatTag(call({ repeatOf }))).toBe(true)
    expect(showsRepeatTag(call({ repeatOf, running: true, outcome: null }))).toBe(true)
    expect(showsRepeatTag(call({ repeatOf, outcome: { kind: 'refused', reason: { kind: 'repeatedCall' } } }))).toBe(
      false,
    )
  })
})
