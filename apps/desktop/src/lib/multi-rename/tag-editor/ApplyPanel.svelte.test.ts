/**
 * The Apply panel: the change list comes before any write, diacritics get a fix and never a silent strip, "Write
 * anyway" re-sends only the would-drop files, "Skip these files" writes nothing for them, and every file left
 * unwritten is listed with why.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, unmount } from 'svelte'
import ApplyPanel from './ApplyPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { PANEL_VIEW, applyReport, asState, buttonNamed, fakeState, press } from './apply-panel.test-helper'

const confirm = vi.hoisted(() => vi.fn((_message: string, _title?: string, _label?: string) => Promise.resolve(true)))
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog: confirm }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  confirm.mockClear()
})

async function mountPanel(fake = fakeState()) {
  const mounted = await mountInto(ApplyPanel, { tags: asState(fake) })
  cleanup = mounted.cleanup
  return { root: mounted.root, fake }
}

const listed = (root: HTMLElement, label: string) =>
  [...(root.querySelector(`[aria-label="${label}"]`)?.querySelectorAll('li') ?? [])].map((li) => li.textContent.trim())

describe('ApplyPanel', () => {
  it('apply_shows_change_list_first', async () => {
    const { root, fake } = await mountPanel()
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenCalledTimes(1)
    expect(fake.apply).not.toHaveBeenCalled()
    expect(root.textContent).toContain('Writing as Audiobook: 2 files will change:')
    expect(listed(root, 'Files that will change')).toEqual(['01.mp3: Album, Title, cover', '02.mp3: Album, extra tags'])
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenCalledWith({ kind: 'audiobook', allowFrameLoss: false, skip: [] }, { after: [] })
  })

  it('a list with nothing in it writes nothing, so ⌘⏎ ⌘⏎ keeps the last Undo', async () => {
    const fake = fakeState()
    fake.changes.mockResolvedValue({ ok: true, value: [] })
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(root.textContent).toContain('Nothing to write')
    expect(buttonNamed(root, 'Write tags').disabled).toBe(true)
    await press(root, 'Back')
    expect(fake.apply).not.toHaveBeenCalled()
  })

  it('Back closes the list without writing', async () => {
    const { root, fake } = await mountPanel()
    await press(root, 'Write tags…')
    await press(root, 'Back')
    expect(root.textContent).not.toContain('will change')
    expect(fake.apply).not.toHaveBeenCalled()
  })

  it('an edit made while the list shows lists again instead of writing', async () => {
    const { root, fake } = await mountPanel()
    await press(root, 'Write tags…')
    fake.view = { ...PANEL_VIEW }
    await press(root, 'Write tags')
    expect(fake.apply).not.toHaveBeenCalled()
    expect(fake.changes).toHaveBeenCalledTimes(2)
  })

  it('the content kind goes with the write, Music when picked', async () => {
    const { root, fake } = await mountPanel()
    await press(root, 'Music')
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenCalledWith({ kind: 'music', allowFrameLoss: false, skip: [] }, { after: [] })
  })

  it('diacritics_warning_offers_fix_not_silent', async () => {
    const { root, fake } = await mountPanel(fakeState({ ...PANEL_VIEW, diacritics: ['album', 'title'] }))
    expect(root.textContent).toContain('Diacritics in: Album, Title')
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    // Writing as the user typed: nothing stripped behind their back.
    expect(fake.tool).not.toHaveBeenCalled()
    await press(root, 'Remove diacritics')
    expect(fake.tool).toHaveBeenCalledWith({ kind: 'stripDiacritics', fields: ['album', 'title'] })
  })

  it('would_drop_dialog_write_anyway_resends_with_allow', async () => {
    const fake = fakeState(PANEL_VIEW, [
      applyReport({ written: ['/b/01.mp3'], wouldDrop: [{ path: '/b/02.mp3', items: ['TSOP'] }] }),
      applyReport({ applyId: 'A2', written: ['/b/02.mp3'] }),
    ])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(root.textContent).toContain('02.mp3: TSOP')
    await press(root, 'Write anyway')
    expect(fake.apply).toHaveBeenCalledTimes(2)
    expect(fake.apply).toHaveBeenLastCalledWith(
      { kind: 'audiobook', allowFrameLoss: true, skip: ['/b/01.mp3', '/b/03.mp3'] },
      { after: [expect.objectContaining({ applyId: 'A1', written: ['/b/01.mp3'] })] },
    )
    expect(root.textContent).toContain('Tags written to 2 files')
  })

  it('would_drop_skip_writes_nothing_for_them', async () => {
    const fake = fakeState(PANEL_VIEW, [
      applyReport({ written: ['/b/01.mp3'], wouldDrop: [{ path: '/b/02.mp3', items: ['TSOP'] }] }),
    ])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    await press(root, 'Skip these files')
    expect(fake.apply).toHaveBeenCalledTimes(1)
    expect(root.textContent).toContain('Tags written to 1 file')
    expect(root.textContent).toContain('Skipped, not written:')
    expect(listed(root, 'Skipped, not written:')).toEqual(['02.mp3'])
  })

  it('failed_rows_listed', async () => {
    const fake = fakeState(PANEL_VIEW, [
      applyReport({
        written: ['/b/01.mp3'],
        failed: [
          { path: '/b/02.mp3', error: { type: 'io', data: ['PermissionDenied', 'denied'] } },
          { path: '/b/03.mp3', error: { type: 'parse', data: 'bad' } },
        ],
      }),
    ])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(listed(root, 'Not written:')).toEqual([
      '02.mp3: the file couldn’t be opened or saved',
      '03.mp3: its tags couldn’t be read',
    ])
    // The OS message stays out of the UI.
    expect(root.textContent).not.toContain('denied')
  })

  it('a file changed on disk is listed, and Read the files again asks before dropping edits', async () => {
    const fake = fakeState(PANEL_VIEW, [applyReport({ written: [], applyId: null, changedOnDisk: ['/b/03.mp3'] })])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(listed(root, 'Changed on disk since it was read, not written:')).toEqual(['03.mp3'])
    confirm.mockResolvedValueOnce(false)
    await press(root, 'Read the files again')
    expect(fake.reload).not.toHaveBeenCalled()
    await press(root, 'Read the files again')
    expect(confirm).toHaveBeenCalledTimes(2)
    expect(fake.reload).toHaveBeenCalledTimes(1)
  })

  it('a write that keeps no undo says so', async () => {
    const fake = fakeState(PANEL_VIEW, [applyReport({ written: ['/b/01.mp3'], undoUnavailable: true })])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(root.textContent).toContain('This write can’t be undone')
  })

  it('the review header names the content kind', async () => {
    const { root } = await mountPanel()
    await press(root, 'Music')
    await press(root, 'Write tags…')
    expect(root.textContent).toContain('Writing as Music: 2 files will change:')
  })

  it('an edit made while the would-drop choice shows lists again instead of writing anyway', async () => {
    const fake = fakeState(PANEL_VIEW, [
      applyReport({ written: ['/b/01.mp3'], wouldDrop: [{ path: '/b/02.mp3', items: ['TSOP'] }] }),
    ])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    fake.view = { ...PANEL_VIEW }
    await press(root, 'Write anyway')
    expect(fake.apply).toHaveBeenCalledTimes(1)
    expect(fake.changes).toHaveBeenCalledTimes(2)
    expect(root.textContent).toContain('will change:')
  })

  it('the re-list keeps the first apply: its outcome shows, and the next write continues it', async () => {
    const first = applyReport({ written: ['/b/01.mp3'], wouldDrop: [{ path: '/b/02.mp3', items: ['TSOP'] }] })
    const fake = fakeState(PANEL_VIEW, [first, applyReport({ applyId: 'A2', written: ['/b/02.mp3'] })])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    fake.view = { ...PANEL_VIEW }
    await press(root, 'Write anyway')
    // Re-listed: the first apply's outcome stays on screen above the new list.
    expect(root.textContent).toContain('Tags written to 1 file')
    expect(root.textContent).toContain('will change:')
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenLastCalledWith(
      { kind: 'audiobook', allowFrameLoss: false, skip: [] },
      { after: [first] },
    )
    expect(root.textContent).toContain('Tags written to 2 files')
  })

  it('Back from a re-list shows the first apply’s outcome instead of dropping it', async () => {
    const fake = fakeState(PANEL_VIEW, [
      applyReport({ written: ['/b/01.mp3'], wouldDrop: [{ path: '/b/02.mp3', items: ['TSOP'] }] }),
    ])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    fake.view = { ...PANEL_VIEW }
    await press(root, 'Write anyway')
    await press(root, 'Back')
    expect(root.textContent).toContain('Tags written to 1 file')
    expect(root.textContent).toContain('Skipped, not written:')
  })

  it('Remove diacritics waits for an edit in flight, then runs', async () => {
    const fake = fakeState({ ...PANEL_VIEW, diacritics: ['album'] })
    const { root } = await mountPanel(fake)
    await press(root, 'Remove diacritics')
    expect(fake.settled.mock.invocationCallOrder[0]).toBeLessThan(fake.tool.mock.invocationCallOrder[0])
  })

  it('a write refused before any file was touched says no file was written', async () => {
    const fake = fakeState()
    fake.apply.mockResolvedValue({ ok: false, error: { kind: 'noAppDataDir' } } as never)
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(root.textContent).toContain('No file was written.')
  })

  it('Undo with unwritten edits says they are lost too', async () => {
    const fake = { ...fakeState(), undoable: { applies: [{ applyId: 'A1', written: 1 }], partial: false } }
    const { root } = await mountPanel(fake)
    confirm.mockResolvedValueOnce(false)
    await press(root, 'Undo tag changes')
    expect(confirm.mock.calls[0][0]).toBe(
      'This puts the tags of the 1 file the last write changed back as they were before it. If that file changed since the write, it’s left as it is. Tag changes you haven’t written to those files yet are lost too.',
    )
  })

  it('an undo that stops halfway says what it restored and what is left, and the write line goes', async () => {
    const fake = fakeState(PANEL_VIEW, [applyReport({ written: ['/b/01.mp3', '/b/02.mp3', '/b/03.mp3'] })])
    fake.undoable = { applies: [{ applyId: 'A1', written: 3 }], partial: false }
    fake.undo.mockImplementation(() => {
      fake.undoable = { applies: [{ applyId: 'A0', written: 2 }], partial: false }
      return Promise.resolve({
        ok: false,
        error: { kind: 'taskFailed' },
        restored: { restored: ['/b/03.mp3'], missing: [], changed: [], failed: [] },
      } as never)
    })
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(root.textContent).toContain('Tags written to 3 files')
    await press(root, 'Undo tag changes')
    expect(root.textContent).not.toContain('Tags written to')
    expect(root.textContent).toContain('Tags restored in 1 file')
    expect(root.textContent).toContain('2 files couldn’t be restored.')
  })

  it('Undo tag changes says a file changed since the write is left alone, before undoing', async () => {
    const fake = { ...fakeState(), undoable: { applies: [{ applyId: 'A1', written: 3 }], partial: false } }
    const { root } = await mountPanel(fake)
    confirm.mockResolvedValueOnce(false)
    await press(root, 'Undo tag changes')
    expect(confirm.mock.calls[0][0]).toContain('A file that changed since the write is left as it is.')
    expect(fake.undo).not.toHaveBeenCalled()
    await press(root, 'Undo tag changes')
    expect(fake.undo).toHaveBeenCalledTimes(1)
  })

  it('a read-only session offers no write', async () => {
    const { root } = await mountPanel(fakeState({ ...PANEL_VIEW, editable: false }))
    expect(buttonNamed(root, 'Write tags…').disabled).toBe(true)
  })

  it('start() twice with nothing to write writes nothing (⌘⏎ ⌘⏎ keeps the last Undo)', async () => {
    const fake = fakeState()
    fake.changes.mockResolvedValue({ ok: true, value: [] })
    const root = document.createElement('div')
    document.body.appendChild(root)
    const panel = mount(ApplyPanel, { target: root, props: { tags: asState(fake) } }) as { start: () => Promise<void> }
    cleanup = () => {
      void unmount(panel)
      root.remove()
    }
    await panel.start()
    await panel.start()
    expect(fake.changes).toHaveBeenCalledTimes(1)
    expect(fake.apply).not.toHaveBeenCalled()
  })

  it('start() lists the changes, and a second start() writes them', async () => {
    const fake = fakeState()
    const root = document.createElement('div')
    document.body.appendChild(root)
    const panel = mount(ApplyPanel, { target: root, props: { tags: asState(fake) } }) as { start: () => Promise<void> }
    cleanup = () => {
      void unmount(panel)
      root.remove()
    }
    await panel.start()
    expect(fake.changes).toHaveBeenCalledTimes(1)
    expect(fake.apply).not.toHaveBeenCalled()
    await panel.start()
    expect(fake.apply).toHaveBeenCalledTimes(1)
  })
})
