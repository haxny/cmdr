/**
 * Plays a fixture into a `TransferTree`: its scripted transfer, plus the `tick`
 * the tree pulls on, the way a session's `progress` would change. The Debug
 * catalog runs it on a timer (looping); the tests step it by hand.
 */

import type { TransferTreeOperation } from '../row-format'
import type { TransferTreeFixture } from './catalog'
import type { ScriptedTransfer } from './scripted-transfer'

/** The backend's `write-progress` cadence. */
const STEP_MS = 200
/** How long a finished fixture rests before it plays again. */
const REST_STEPS = 15

export interface FixturePlayer {
  /** The props a `TransferTree` takes, kept current. */
  readonly props: { source: ScriptedTransfer; tick: number; operation: TransferTreeOperation }
  readonly transfer: ScriptedTransfer
  /** Plays `count` progress ticks. */
  step: (count?: number) => void
  /** Runs it live until `stop`, starting over (a new tree) a moment after it finishes. */
  play: () => void
  stop: () => void
}

export function createFixturePlayer(fixture: TransferTreeFixture): FixturePlayer {
  const props = $state({ source: fixture.create(), tick: 0, operation: fixture.operation })
  let timer: ReturnType<typeof setInterval> | null = null
  let rest = 0

  function step(count = 1): void {
    for (let i = 0; i < count; i++) props.source.advance()
    props.tick++
  }

  return {
    props,
    get transfer() {
      return props.source
    },
    step,
    play() {
      if (timer !== null) return
      timer = setInterval(() => {
        if (!props.source.finished) {
          step()
        } else if (++rest > REST_STEPS) {
          rest = 0
          props.source = fixture.create()
          props.tick++
        }
      }, STEP_MS)
    },
    stop() {
      if (timer !== null) clearInterval(timer)
      timer = null
    },
  }
}
