/** The red button's close request (`routes/tags/+page.svelte`): asks about unwritten edits, never throws, never strands. */
import { describe, expect, it, vi } from 'vitest'

const warn = vi.hoisted(() => vi.fn())
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

import { onTagsCloseRequested } from './tags-close-request'

function request() {
  return { preventDefault: vi.fn() }
}

describe('onTagsCloseRequested', () => {
  it('lets the close through when the body is gone (null or not yet mounted), without throwing', () => {
    for (const body of [null, undefined]) {
      const event = request()
      expect(() => {
        onTagsCloseRequested(body, event)
      }).not.toThrow()
      expect(event.preventDefault).not.toHaveBeenCalled()
    }
  })

  it('lets the close through once the window is on its way out', () => {
    const body = { isEnding: () => true, requestClose: vi.fn() }
    const event = request()
    onTagsCloseRequested(body, event)
    expect(event.preventDefault).not.toHaveBeenCalled()
    expect(body.requestClose).not.toHaveBeenCalled()
  })

  it('holds the close and asks the body (the discard question) while the window is live', () => {
    const body = { isEnding: () => false, requestClose: vi.fn() }
    const event = request()
    onTagsCloseRequested(body, event)
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(body.requestClose).toHaveBeenCalledOnce()
  })

  it('lets the close through, and logs, when the body throws before it could ask', () => {
    const body = {
      isEnding: () => {
        throw new Error('unmounted')
      },
      requestClose: vi.fn(),
    }
    const event = request()
    expect(() => {
      onTagsCloseRequested(body, event)
    }).not.toThrow()
    expect(event.preventDefault).not.toHaveBeenCalled()
    expect(warn).toHaveBeenCalled()
  })
})
