/**
 * The card a running answer shows when it passed the user's spend threshold: the amount so far,
 * the estimate's caveat, and Keep going and Stop wired to their handlers. Its a11y lives in
 * `AskCmdrSpendPause.a11y.test.ts`.
 */

import { describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'

import AskCmdrSpendPause from './AskCmdrSpendPause.svelte'

function render(): { target: HTMLElement; onKeepGoing: ReturnType<typeof vi.fn>; onStop: ReturnType<typeof vi.fn> } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onKeepGoing = vi.fn()
  const onStop = vi.fn()
  mount(AskCmdrSpendPause, { target, props: { costMicros: 2_140_000, onKeepGoing, onStop } })
  flushSync()
  return { target, onKeepGoing, onStop }
}

describe('AskCmdrSpendPause', () => {
  it('says what the answer has used so far', () => {
    expect(render().target.textContent).toContain('$2.14')
  })

  it('wires Keep going and Stop', () => {
    const { target, onKeepGoing, onStop } = render()
    const [keepGoing, stop] = [...target.querySelectorAll<HTMLButtonElement>('button')]
    keepGoing.click()
    stop.click()
    expect(onKeepGoing).toHaveBeenCalledOnce()
    expect(onStop).toHaveBeenCalledOnce()
  })
})
