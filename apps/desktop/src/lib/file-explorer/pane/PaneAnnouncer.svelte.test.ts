/**
 * The pane's one polite live region. A region inserted on demand announces nothing, so it's
 * mounted with the pane and empty; each message clears it first and lands a frame later, so
 * the same text twice in a row is a change both times.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import PaneAnnouncer from './PaneAnnouncer.svelte'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  document.body.innerHTML = ''
  vi.useRealTimers()
})

function setup(): { region: HTMLElement; announce: (message: string) => void } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(PaneAnnouncer, { target })
  const region = target.querySelector<HTMLElement>('[data-pane-announcer]')
  if (!region) throw new Error('PaneAnnouncer rendered no region')
  return { region, announce: (component as unknown as { announce: (m: string) => void }).announce }
}

describe('PaneAnnouncer', () => {
  it('is a live region from mount, before its first message', () => {
    const { region } = setup()
    expect(region.getAttribute('aria-live')).toBe('polite')
    expect(region.textContent).toBe('')
  })

  it('says a message one frame after clearing', () => {
    const { region, announce } = setup()
    announce('Left pane, vo-test')
    flushSync()
    expect(region.textContent).toBe('')
    vi.advanceTimersToNextFrame()
    flushSync()
    expect(region.textContent).toBe('Left pane, vo-test')
  })

  it('says the same message again when asked twice', () => {
    const { region, announce } = setup()
    const seen: string[] = []
    for (let i = 0; i < 2; i++) {
      announce('Left pane, vo-test')
      flushSync()
      seen.push(region.textContent)
      vi.advanceTimersToNextFrame()
      flushSync()
      seen.push(region.textContent)
    }
    expect(seen).toEqual(['', 'Left pane, vo-test', '', 'Left pane, vo-test'])
  })

  it('says only the newest of two messages asked within a frame', () => {
    const { region, announce } = setup()
    announce('Left pane, a')
    announce('Right pane, b')
    vi.advanceTimersByTime(1000)
    flushSync()
    expect(region.textContent).toBe('Right pane, b')
  })
})
