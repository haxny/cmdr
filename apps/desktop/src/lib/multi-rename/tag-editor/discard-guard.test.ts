/** The discard guard: unwritten tag edits are never lost without the "Discard tag changes?" question (Review Focus 4). */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { TagEditorState } from './tag-editor-state.svelte'

const confirmDialog = vi.hoisted(() => vi.fn())
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog }))
const warn = vi.hoisted(() => vi.fn())
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

import { createDiscardGuard } from './discard-guard'

function tagsWith(
  view: { dirty: boolean } | null,
  over: { busy?: boolean; settled?: () => Promise<void> } = {},
): TagEditorState {
  return { view, busy: false, settled: () => Promise.resolve(), ...over } as unknown as TagEditorState
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

beforeEach(() => {
  vi.resetAllMocks()
})

describe('createDiscardGuard', () => {
  it('with the Tags tab off it closes at once and never asks', () => {
    const onClose = vi.fn()
    const guard = createDiscardGuard(null, onClose)
    guard.close()
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(guard.dirty).toBe(false)
    expect(confirmDialog).not.toHaveBeenCalled()
  })

  it('closes at once when nothing is unwritten (or the tab never opened)', () => {
    const onClose = vi.fn()
    createDiscardGuard(tagsWith({ dirty: false }), onClose).close()
    createDiscardGuard(tagsWith(null), onClose).close()
    expect(onClose).toHaveBeenCalledTimes(2)
    expect(confirmDialog).not.toHaveBeenCalled()
  })

  it('close_with_unwritten_changes_asks: Discard closes', async () => {
    confirmDialog.mockResolvedValue(true)
    const onClose = vi.fn()
    createDiscardGuard(tagsWith({ dirty: true }), onClose).close()
    expect(onClose).not.toHaveBeenCalled()
    await settle()
    expect(confirmDialog).toHaveBeenCalledWith(
      expect.stringContaining('haven’t been written'),
      'Discard tag changes?',
      'Discard',
    )
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('keeping the edits leaves the sheet open', async () => {
    confirmDialog.mockResolvedValue(false)
    const onClose = vi.fn()
    createDiscardGuard(tagsWith({ dirty: true }), onClose).close()
    await settle()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('asks one question at a time', async () => {
    let answer: (discard: boolean) => void = () => {}
    confirmDialog.mockImplementation(
      () =>
        new Promise<boolean>((resolve) => {
          answer = resolve
        }),
    )
    const onClose = vi.fn()
    const guard = createDiscardGuard(tagsWith({ dirty: true }), onClose)
    guard.close()
    guard.close()
    await settle()
    expect(confirmDialog).toHaveBeenCalledTimes(1)
    answer(false)
    await settle()
    guard.close()
    await settle()
    expect(confirmDialog).toHaveBeenCalledTimes(2)
  })

  it('a question that cannot be asked keeps the sheet open and is logged', async () => {
    confirmDialog.mockRejectedValue(new Error('no dialog'))
    const onClose = vi.fn()
    createDiscardGuard(tagsWith({ dirty: true }), onClose).close()
    await settle()
    expect(onClose).not.toHaveBeenCalled()
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('confirm() answers true without asking when nothing is unwritten, else asks', async () => {
    expect(await createDiscardGuard(tagsWith({ dirty: false }), vi.fn()).confirm()).toBe(true)
    expect(confirmDialog).not.toHaveBeenCalled()
    confirmDialog.mockResolvedValue(false)
    expect(await createDiscardGuard(tagsWith({ dirty: true }), vi.fn()).confirm()).toBe(false)
  })

  it('an edit still on its way counts as unwritten: the question waits for it, then decides on what it left', async () => {
    let finish: () => void = () => {}
    const tags = {
      view: { dirty: false } as { dirty: boolean },
      busy: true,
      settled: () =>
        new Promise<void>((resolve) => {
          finish = resolve
        }),
    } as unknown as TagEditorState
    confirmDialog.mockResolvedValue(false)
    const onClose = vi.fn()
    const guard = createDiscardGuard(tags, onClose)
    expect(guard.dirty).toBe(true)
    guard.close()
    await settle()
    // Closing waits for the edit; nothing has closed or been asked yet.
    expect(onClose).not.toHaveBeenCalled()
    expect(confirmDialog).not.toHaveBeenCalled()
    // The edit landed and left the tags unwritten: now it asks.
    ;(tags.view as { dirty: boolean }).dirty = true
    finish()
    await settle()
    expect(confirmDialog).toHaveBeenCalledTimes(1)
    expect(onClose).not.toHaveBeenCalled()
  })

  it('an edit on its way that leaves nothing unwritten closes without a question', async () => {
    const tags = tagsWith({ dirty: false }, { busy: true })
    const onClose = vi.fn()
    createDiscardGuard(tags, onClose).close()
    await settle()
    expect(confirmDialog).not.toHaveBeenCalled()
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
