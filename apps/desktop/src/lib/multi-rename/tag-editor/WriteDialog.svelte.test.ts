/**
 * The Write dialog: ⌘S's list of changes, written on confirm; ⌘Z's undo question, closed again on a "no"; and no
 * way out while a write is on its way.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'

// ⌘ is the Command key, as in the app (the registry reads `isMacOS()` at load).
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)', language: 'en' })
})

const confirm = vi.hoisted(() => vi.fn())
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog: confirm }))
vi.mock('#lib/app-status-store.js', () => ({
  loadTagWriteKind: vi.fn(() => Promise.resolve(null)),
  saveTagWriteKind: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', async (original) => ({
  ...(await original<object>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import WriteDialog from './WriteDialog.svelte'
import { applyReport, asState, buttonNamed, fakeState, settle } from './apply-panel.test-helper'
import { mountInto } from './tag-cell.test-helper'
import type { LastTagApply } from './last-tag-apply'
import type { TagApplyReport } from '#lib/tauri-commands/index.js'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountDialog(mode: 'write' | 'undo', state = fakeState(), continuing?: TagApplyReport[]) {
  const onClose = vi.fn()
  const onWriting = vi.fn()
  const mounted = await mountInto(WriteDialog, { tags: asState(state), mode, onWriting, onClose, continuing })
  cleanup = mounted.cleanup
  await settle()
  return { state, onClose, onWriting }
}

const escape = () =>
  document
    .querySelector('[role="dialog"]')
    ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))

describe('WriteDialog', () => {
  it('opens listing the changes, and writes them on confirm', async () => {
    const { state, onWriting } = await mountDialog(
      'write',
      fakeState(undefined, [applyReport({ written: ['/b/01.mp3'] })]),
    )
    expect(state.changes).toHaveBeenCalledTimes(1)
    expect(document.body.textContent).toContain('01.mp3')
    expect(state.apply).not.toHaveBeenCalled()
    buttonNamed(document.body, 'Write tags').click()
    await settle()
    expect(onWriting).toHaveBeenCalledTimes(1)
    expect(state.apply).toHaveBeenCalledTimes(1)
  })

  it('continuing a press (an edit arrived during ⌘⏎’s would-drop choice) lists again, and its write continues it', async () => {
    const earlier = [applyReport({ written: ['/b/02.mp3'], wouldDrop: [{ path: '/b/01.mp3', items: ['TSOP'] }] })]
    const { state } = await mountDialog('write', fakeState(), earlier)
    expect(state.changes).toHaveBeenCalledTimes(1)
    buttonNamed(document.body, 'Write tags').click()
    await settle()
    expect(state.apply).toHaveBeenCalledWith(
      { kind: 'audiobook', allowFrameLoss: false, skip: [], allowRemovals: [], allowSideFiles: [] },
      { after: earlier },
    )
  })

  it('⌘⏎ confirms the list', async () => {
    const { state } = await mountDialog('write')
    document
      .querySelector('[role="dialog"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }))
    await settle()
    expect(state.apply).toHaveBeenCalledTimes(1)
  })

  it('Escape closes it once the write answered', async () => {
    const { onClose } = await mountDialog('write')
    escape()
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('can’t be closed while the write is on its way', async () => {
    const state = fakeState()
    state.apply.mockImplementation(() => new Promise(() => {}))
    const { onClose } = await mountDialog('write', state)
    buttonNamed(document.body, 'Write tags').click()
    await settle()
    escape()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('undo asks first; a “no” closes the dialog, a “yes” undoes and stays to say so', async () => {
    const state = fakeState()
    state.undoable = { applies: [{ written: 1 }] } as unknown as LastTagApply
    confirm.mockResolvedValueOnce(false)
    const first = await mountDialog('undo', state)
    expect(confirm).toHaveBeenCalledTimes(1)
    expect(state.undo).not.toHaveBeenCalled()
    expect(first.onClose).toHaveBeenCalledTimes(1)
    cleanup?.()

    confirm.mockResolvedValueOnce(true)
    const second = await mountDialog('undo', state)
    expect(state.undo).toHaveBeenCalledTimes(1)
    expect(second.onClose).not.toHaveBeenCalled()
  })
})
