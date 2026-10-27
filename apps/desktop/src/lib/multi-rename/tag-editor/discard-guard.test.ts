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

function tagsWith(view: { dirty: boolean } | null): TagEditorState {
  return { view } as unknown as TagEditorState
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
    expect(confirmDialog).toHaveBeenCalledTimes(1)
    answer(false)
    await settle()
    guard.close()
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
})
