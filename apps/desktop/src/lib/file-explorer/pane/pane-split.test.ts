import { describe, it, expect } from 'vitest'
import {
  BURST_GAP_MS,
  clampSplitPercent,
  createSplitNudger,
  MAX_SPLIT_PERCENT,
  MIN_SPLIT_PERCENT,
  stepSplit,
  type SplitBurst,
  type SplitDirection,
} from './pane-split'

/** Runs presses through `stepSplit` from `start`, returning every resulting percent. */
function press(start: number, presses: { direction: SplitDirection; at: number; freshPress: boolean }[]): number[] {
  let current = start
  let burst: SplitBurst | null = null
  const out: number[] = []
  for (const p of presses) {
    const result = stepSplit(burst, { current, direction: p.direction, now: p.at, freshPress: p.freshPress })
    current = result.percent
    burst = result.burst
    out.push(current)
  }
  return out
}

/** A held key: one fresh press, then auto-repeats every `interval` ms after an initial delay. */
function held(direction: SplitDirection, count: number, interval = 30, initialDelay = 225) {
  return Array.from({ length: count }, (_, i) => ({
    direction,
    at: i === 0 ? 0 : initialDelay + (i - 1) * interval,
    freshPress: i === 0,
  }))
}

describe('clampSplitPercent', () => {
  it('keeps the split within the drag limits', () => {
    expect(clampSplitPercent(10)).toBe(MIN_SPLIT_PERCENT)
    expect(clampSplitPercent(90)).toBe(MAX_SPLIT_PERCENT)
    expect(clampSplitPercent(42)).toBe(42)
  })
})

describe('stepSplit', () => {
  it('nudges by one percent on a single press', () => {
    expect(press(60, [{ direction: 'right', at: 0, freshPress: true }])).toEqual([61])
    expect(press(40, [{ direction: 'left', at: 0, freshPress: true }])).toEqual([39])
  })

  it('grows the step while the key is held, up to a cap', () => {
    const percents = press(50, held('right', 6)).map((p) => Math.round(p * 100) / 100)
    const steps = percents.map((p, i) => Math.round((p - (i === 0 ? 50 : percents[i - 1])) * 100) / 100)
    expect(steps[0]).toBe(1)
    for (let i = 1; i < steps.length; i++) expect(steps[i]).toBeGreaterThanOrEqual(steps[i - 1])
    expect(steps[steps.length - 1]).toBeGreaterThan(5)
    expect(Math.max(...steps)).toBeLessThanOrEqual(8)
  })

  it('keeps accelerating across quick taps, and starts small again after a pause', () => {
    const taps = press(60, [
      { direction: 'right', at: 0, freshPress: true },
      { direction: 'right', at: 100, freshPress: true },
      { direction: 'right', at: 100 + BURST_GAP_MS + 1, freshPress: true },
    ])
    expect(taps[1] - taps[0]).toBeGreaterThan(1)
    expect(taps[2] - taps[1]).toBe(1)
  })

  it('starts small again when the direction flips', () => {
    const percents = press(60, [
      { direction: 'right', at: 0, freshPress: true },
      { direction: 'right', at: 50, freshPress: false },
      { direction: 'left', at: 100, freshPress: true },
    ])
    expect(percents[1] - percents[2]).toBe(1)
  })

  it('clamps to the drag limits', () => {
    expect(press(74.5, held('right', 3)).at(-1)).toBe(MAX_SPLIT_PERCENT)
    expect(press(25.5, held('left', 3)).at(-1)).toBe(MIN_SPLIT_PERCENT)
  })

  it('stops at exactly 50% when heading through the middle, for the rest of a held press', () => {
    expect(press(40, held('right', 20))).toSatisfy((ps: number[]) => ps.at(-1) === 50 && ps.includes(50))
    expect(press(60, held('left', 20)).at(-1)).toBe(50)
  })

  it('stays at 50% even when the first auto-repeat comes after a long initial delay', () => {
    // One press lands on 50, then the OS waits well past the burst gap before repeating.
    const percents = press(49.5, held('right', 4, 30, 600))
    expect(percents).toEqual([50, 50, 50, 50])
  })

  it('moves past 50% on a fresh press', () => {
    const percents = press(49.5, [
      { direction: 'right', at: 0, freshPress: true },
      { direction: 'right', at: 40, freshPress: true },
    ])
    // A quick re-tap still accelerates (1.5%), but it no longer holds at the middle.
    expect(percents).toEqual([50, 51.5])
  })

  it('moves freely away from the middle', () => {
    expect(press(50, held('left', 2))).toEqual([49, 47.5])
    expect(press(55, [{ direction: 'right', at: 0, freshPress: true }])).toEqual([56])
  })
})

describe('createSplitNudger', () => {
  function setup(start: number) {
    let percent = start
    let clock = 0
    const persisted: number[] = []
    const nudger = createSplitNudger({
      now: () => clock,
      getPercent: () => percent,
      setPercent: (p) => {
        percent = p
      },
      persist: (p) => persisted.push(p),
    })
    return {
      nudger,
      advance: (ms: number) => {
        clock += ms
      },
      percent: () => percent,
      persisted,
    }
  }

  it('applies and persists each step', () => {
    const t = setup(60)
    t.nudger.nudge('left')
    expect(t.percent()).toBe(59)
    expect(t.persisted).toEqual([59])
  })

  it('treats presses with no key release in between as one held press', () => {
    const t = setup(49.5)
    t.nudger.nudge('right')
    t.advance(500)
    t.nudger.nudge('right')
    expect(t.percent()).toBe(50)
  })

  it('treats a press after a key release as fresh', () => {
    const t = setup(49.5)
    t.nudger.nudge('right')
    t.nudger.markReleased()
    t.advance(500)
    t.nudger.nudge('right')
    expect(t.percent()).toBe(51)
  })

  it('centers the split and persists it', () => {
    const t = setup(70)
    t.nudger.center()
    expect(t.percent()).toBe(50)
    expect(t.persisted).toEqual([50])
  })
})
