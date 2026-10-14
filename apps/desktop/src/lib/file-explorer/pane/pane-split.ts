/**
 * The pane divider's keyboard moves (`pane.moveDividerLeft` / `Right` /
 * `pane.resetDivider`) and the drag's clamp. `stepSplit` is the pure core;
 * `createSplitNudger` adds the clock and the key-release tracking around it.
 * Rules and the why behind the numbers: `DETAILS.md` § "Pane divider from the keyboard".
 */

/** The split's limits, shared with the drag in `PaneResizer.svelte`. */
export const MIN_SPLIT_PERCENT = 25
export const MAX_SPLIT_PERCENT = 75
export const CENTER_SPLIT_PERCENT = 50

/** Presses closer together than this keep accelerating, even with the key released in between. */
export const BURST_GAP_MS = 150
const FIRST_STEP_PERCENT = 1
const STEP_GROWTH = 1.5
const MAX_STEP_PERCENT = 8

export type SplitDirection = 'left' | 'right'

/** The run of presses the current one belongs to. */
export interface SplitBurst {
  direction: SplitDirection
  /** When the last press of the burst landed. */
  at: number
  /** Presses so far in the burst, so the next step is `FIRST_STEP_PERCENT × STEP_GROWTH^presses`. */
  presses: number
  /** The burst hit the middle on its way through, so it holds there until a fresh press. */
  heldAtCenter: boolean
}

export interface SplitStepInput {
  /** The left pane's width now, in percent. */
  current: number
  direction: SplitDirection
  now: number
  /** The key went up since the previous press. False for an auto-repeat of a held key. */
  freshPress: boolean
}

export function clampSplitPercent(percent: number): number {
  return Math.max(MIN_SPLIT_PERCENT, Math.min(MAX_SPLIT_PERCENT, percent))
}

/**
 * One keyboard nudge of the divider. A held key, or quick taps, accelerate from
 * a 1% step; heading through the middle stops at exactly 50% and holds there
 * until the user lets go and presses again.
 */
export function stepSplit(burst: SplitBurst | null, input: SplitStepInput): { percent: number; burst: SplitBurst } {
  const { current, direction, now, freshPress } = input
  // A held key is one burst however long the OS waits before its first repeat
  // (225 ms at the shortest), so only a released key goes by the gap.
  const continues = burst !== null && burst.direction === direction && (!freshPress || now - burst.at <= BURST_GAP_MS)
  const presses = continues ? burst.presses : 0
  const heldAtCenter = continues && !freshPress && burst.heldAtCenter

  if (heldAtCenter && current === CENTER_SPLIT_PERCENT) {
    return { percent: current, burst: { direction, at: now, presses: presses + 1, heldAtCenter } }
  }

  const step = Math.min(FIRST_STEP_PERCENT * STEP_GROWTH ** presses, MAX_STEP_PERCENT)
  const target = clampSplitPercent(direction === 'right' ? current + step : current - step)
  const crossesCenter =
    direction === 'right'
      ? current < CENTER_SPLIT_PERCENT && target >= CENTER_SPLIT_PERCENT
      : current > CENTER_SPLIT_PERCENT && target <= CENTER_SPLIT_PERCENT
  return {
    percent: crossesCenter ? CENTER_SPLIT_PERCENT : target,
    burst: { direction, at: now, presses: presses + 1, heldAtCenter: crossesCenter },
  }
}

export interface SplitNudgerDeps {
  now: () => number
  getPercent: () => number
  setPercent: (percent: number) => void
  /** Saves the split, the same call the drag makes on mouse-up. */
  persist: (percent: number) => void
}

/**
 * Owns the burst between presses. Commands reach it without their key event, so
 * a fresh press is read from `keyup`: `listenForKeyRelease` marks one on any key
 * going up (and on window blur, where the `keyup` goes elsewhere).
 */
export function createSplitNudger(deps: SplitNudgerDeps) {
  let burst: SplitBurst | null = null
  let released = true

  function markReleased(): void {
    released = true
  }

  return {
    nudge(direction: SplitDirection): void {
      const result = stepSplit(burst, { current: deps.getPercent(), direction, now: deps.now(), freshPress: released })
      burst = result.burst
      released = false
      deps.setPercent(result.percent)
      deps.persist(result.percent)
    },

    center(): void {
      burst = null
      deps.setPercent(CENTER_SPLIT_PERCENT)
      deps.persist(CENTER_SPLIT_PERCENT)
    },

    markReleased,

    /** Installs the release listeners; returns their cleanup. */
    listenForKeyRelease(target: Window): () => void {
      target.addEventListener('keyup', markReleased, true)
      target.addEventListener('blur', markReleased)
      return () => {
        target.removeEventListener('keyup', markReleased, true)
        target.removeEventListener('blur', markReleased)
      }
    },
  }
}
