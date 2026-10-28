/** The red button's close request (`routes/tags/+page.svelte`): asks about unwritten edits, never throws, never strands. */
import { describe, expect, it, vi } from 'vitest'

const warn = vi.hoisted(() => vi.fn())
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

const ipc = vi.hoisted(() => ({
  tagsWindowMayClose: vi.fn(),
  releaseTagsSession: vi.fn(() => Promise.resolve()),
  closeTagEditor: vi.fn(() => Promise.resolve()),
  closeMultiRename: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)
const closeSelfWindow = vi.hoisted(() => vi.fn())
vi.mock('#lib/child-window-close.js', () => ({ closeSelfWindow }))
const confirmDialog = vi.hoisted(() => vi.fn())
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog }))

import { closeAfterBodyFailed, closeBrokenWindow, onTagsCloseRequested } from './tags-close-request'

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

  it('keeps the window up, and logs, when the close question itself throws', () => {
    const body = {
      isEnding: () => false,
      requestClose: () => {
        throw new Error('no dialog')
      },
    }
    const event = request()
    warn.mockClear()
    expect(() => {
      onTagsCloseRequested(body, event)
    }).not.toThrow()
    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(warn).toHaveBeenCalled()
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

/** The error boundary's close: the edits live in the backend session, so only a clean one may go unasked. */
describe('closeAfterBodyFailed', () => {
  it('closes the window when nothing would be lost', async () => {
    ipc.tagsWindowMayClose.mockReset().mockResolvedValue(true)
    closeSelfWindow.mockReset()
    await closeAfterBodyFailed('multi-rename-1')
    expect(ipc.tagsWindowMayClose).toHaveBeenCalledWith('multi-rename-1')
    expect(closeSelfWindow).toHaveBeenCalledOnce()
  })

  it('keeps the window (and its held session) when edits are unwritten or a write runs', async () => {
    ipc.tagsWindowMayClose.mockReset().mockResolvedValue(false)
    closeSelfWindow.mockReset()
    await closeAfterBodyFailed('multi-rename-1')
    expect(closeSelfWindow).not.toHaveBeenCalled()
  })

  it('keeps the window when the backend cannot say', async () => {
    ipc.tagsWindowMayClose.mockReset().mockRejectedValue(new Error('ipc'))
    closeSelfWindow.mockReset()
    await expect(closeAfterBodyFailed('multi-rename-1')).resolves.toBeUndefined()
    expect(closeSelfWindow).not.toHaveBeenCalled()
  })
})

/** A body that stopped reloading: its Close (and the red button) asks as ⌘W does, then ends the session. */
describe('closeBrokenWindow', () => {
  function reset(mayClose: boolean | Error, discard = false) {
    ipc.tagsWindowMayClose.mockReset()
    if (mayClose instanceof Error) ipc.tagsWindowMayClose.mockRejectedValue(mayClose)
    else ipc.tagsWindowMayClose.mockResolvedValue(mayClose)
    confirmDialog.mockReset().mockResolvedValue(discard)
    closeSelfWindow.mockReset()
    ipc.releaseTagsSession.mockClear()
    ipc.closeMultiRename.mockClear()
  }

  it('closes at once, ending the session, when nothing would be lost', async () => {
    reset(true)
    await expect(closeBrokenWindow('multi-rename-1')).resolves.toBe(true)
    expect(confirmDialog).not.toHaveBeenCalled()
    expect(ipc.releaseTagsSession).toHaveBeenCalledWith('multi-rename-1')
    expect(ipc.closeMultiRename).toHaveBeenCalledWith('multi-rename-1')
    expect(closeSelfWindow).toHaveBeenCalledOnce()
  })

  it('asks the discard question when edits are unwritten, and stays on Cancel', async () => {
    reset(false, false)
    await expect(closeBrokenWindow('multi-rename-1')).resolves.toBe(false)
    expect(confirmDialog).toHaveBeenCalledWith(
      'You changed tags that haven’t been written to the files yet. Closing now loses those changes. Cancel keeps editing.',
      'Discard tag changes?',
      'Discard',
    )
    expect(ipc.releaseTagsSession).not.toHaveBeenCalled()
    expect(closeSelfWindow).not.toHaveBeenCalled()
  })

  it('closes on Discard, and asks too when the backend cannot say', async () => {
    reset(false, true)
    await expect(closeBrokenWindow('multi-rename-1')).resolves.toBe(true)
    expect(closeSelfWindow).toHaveBeenCalledOnce()
    reset(new Error('ipc'), false)
    await expect(closeBrokenWindow('multi-rename-1')).resolves.toBe(false)
    expect(confirmDialog).toHaveBeenCalledOnce()
    expect(closeSelfWindow).not.toHaveBeenCalled()
  })
})
