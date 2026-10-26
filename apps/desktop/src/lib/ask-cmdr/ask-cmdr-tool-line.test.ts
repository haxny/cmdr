/**
 * The shared readings of a tool line's typed facts.
 */

import { describe, expect, it } from 'vitest'
import { outcomeOk, subjectPath } from './ask-cmdr-tool-line'

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

describe('subjectPath', () => {
  it('names a folder or the first file, and nothing for other subjects', () => {
    expect(subjectPath({ kind: 'folder', path: '/a' })).toBe('/a')
    expect(subjectPath({ kind: 'files', firstPath: '/a/b.txt', count: 3 })).toBe('/a/b.txt')
    expect(subjectPath({ kind: 'drives' })).toBeNull()
    expect(subjectPath({ kind: 'search', query: 'x', scopePaths: ['/a'] })).toBeNull()
  })
})
