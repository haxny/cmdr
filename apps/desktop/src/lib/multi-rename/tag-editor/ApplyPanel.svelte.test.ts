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
    expect(root.textContent).toContain('2 files will change:')
    expect(listed(root, 'Files that will change')).toEqual(['01.mp3: Album, Title, cover', '02.mp3: Album, extra tags'])
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenCalledWith({ kind: 'audiobook', allowFrameLoss: false, skip: [] })
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
    expect(fake.apply).toHaveBeenCalledWith({ kind: 'music', allowFrameLoss: false, skip: [] })
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
      { continuing: true },
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

  it('Undo tag changes warns that later changes are overwritten before undoing', async () => {
    const fake = { ...fakeState(), undoable: { applyIds: ['A1'], written: 3 } }
    const { root } = await mountPanel(fake)
    confirm.mockResolvedValueOnce(false)
    await press(root, 'Undo tag changes')
    expect(confirm.mock.calls[0][0]).toContain('Any change made to those files since the write is overwritten.')
    expect(fake.undo).not.toHaveBeenCalled()
    await press(root, 'Undo tag changes')
    expect(fake.undo).toHaveBeenCalledTimes(1)
  })

  it('a read-only session offers no write', async () => {
    const { root } = await mountPanel(fakeState({ ...PANEL_VIEW, editable: false }))
    expect(buttonNamed(root, 'Write tags…').disabled).toBe(true)
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
