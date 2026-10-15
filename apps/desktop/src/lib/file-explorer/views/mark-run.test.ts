import { describe, it, expect } from 'vitest'
import { markRunEdges } from './mark-run'

describe('markRunEdges', () => {
  it('rounds both ends of a lone marked row', () => {
    expect(markRunEdges(5, new Set([5]))).toEqual({ isRunStart: true, isRunEnd: true })
  })

  it('rounds only the outer ends of a run, continuous in between', () => {
    const marked = new Set([3, 4, 5])
    expect(markRunEdges(3, marked)).toEqual({ isRunStart: true, isRunEnd: false })
    expect(markRunEdges(4, marked)).toEqual({ isRunStart: false, isRunEnd: false })
    expect(markRunEdges(5, marked)).toEqual({ isRunStart: false, isRunEnd: true })
  })

  it('reads neighbors from the marked set, so a run crossing the rendered window edge stays open', () => {
    // Rows 0..99 marked, only 40..59 rendered: row 40 and row 59 are mid-run.
    const marked = new Set(Array.from({ length: 100 }, (_, i) => i))
    expect(markRunEdges(40, marked)).toEqual({ isRunStart: false, isRunEnd: false })
    expect(markRunEdges(59, marked)).toEqual({ isRunStart: false, isRunEnd: false })
  })

  it('treats the first and last list rows as run ends', () => {
    const marked = new Set([0, 1])
    expect(markRunEdges(0, marked).isRunStart).toBe(true)
    expect(markRunEdges(1, marked).isRunEnd).toBe(true)
  })

  it('breaks runs at Brief column boundaries', () => {
    // Four rows per column: index 3 ends column 0, index 4 starts column 1.
    const marked = new Set([2, 3, 4, 5])
    expect(markRunEdges(2, marked, 4)).toEqual({ isRunStart: true, isRunEnd: false })
    expect(markRunEdges(3, marked, 4)).toEqual({ isRunStart: false, isRunEnd: true })
    expect(markRunEdges(4, marked, 4)).toEqual({ isRunStart: true, isRunEnd: false })
    expect(markRunEdges(5, marked, 4)).toEqual({ isRunStart: false, isRunEnd: true })
  })

  it('keeps a run open across what would be a column boundary in Full view', () => {
    const marked = new Set([3, 4])
    expect(markRunEdges(3, marked)).toEqual({ isRunStart: true, isRunEnd: false })
    expect(markRunEdges(4, marked)).toEqual({ isRunStart: false, isRunEnd: true })
  })
})
