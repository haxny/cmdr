/**
 * Every press in the Apply panel answers where the user looks: it acts, shows that it's working, or says why not,
 * next to its buttons. Regression anchor (2026-10-27): on a 22-file audiobook both write buttons seemed to do
 * nothing, because a refusal or a still-running call ended the press without a word in the panel.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import ApplyPanel from './ApplyPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { PANEL_VIEW, applyReport, asState, buttonNamed, fakeState, press, settle } from './apply-panel.test-helper'
import { WAITING_SHOWN_AFTER_MS, WRITE_WAIT_LIMIT_MS } from './write-wait'

vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog: vi.fn(() => Promise.resolve(true)) }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountPanel(fake = fakeState()) {
  const mounted = await mountInto(ApplyPanel, { tags: asState(fake) })
  cleanup = mounted.cleanup
  return { root: mounted.root, fake }
}

/** The panel's own words, where the buttons are. */
const panelText = (root: HTMLElement): string => root.querySelector('section')?.textContent ?? ''

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** A `settled()` that answers when the test says. */
function heldSettle() {
  let answer: (done: boolean) => void = () => {}
  const held = new Promise<boolean>((resolve) => {
    answer = resolve
  })
  return { held, answer }
}

describe('ApplyPanel answers every press', () => {
  it('Write tags… waits visibly for the last change, keeps waiting, and lists once it lands', async () => {
    const fake = fakeState()
    const { held, answer } = heldSettle()
    fake.settled.mockReturnValueOnce(held)
    const { root } = await mountPanel(fake)

    buttonNamed(root, 'Write tags…').click()
    await pause(WAITING_SHOWN_AFTER_MS + 50)
    expect(panelText(root)).toContain('Finishing the last change…')
    expect(fake.settled).toHaveBeenCalledWith(WRITE_WAIT_LIMIT_MS)
    expect(fake.changes).not.toHaveBeenCalled()

    answer(true)
    await settle()
    expect(fake.changes).toHaveBeenCalledTimes(1)
    expect(panelText(root)).toContain('2 files will change')
    expect(panelText(root)).not.toContain('Finishing the last change…')
  })

  it('a wait that gives up says the files are still being read, and the button stays pressable', async () => {
    const fake = fakeState()
    fake.settled.mockResolvedValueOnce(false)
    const { root } = await mountPanel(fake)

    await press(root, 'Write tags…')

    expect(panelText(root)).toContain('Cmdr is still reading the files. Try again in a moment.')
    expect(buttonNamed(root, 'Write tags…').disabled).toBe(false)
    expect(fake.changes).not.toHaveBeenCalled()
    await press(root, 'Write tags…')
    expect(fake.changes).toHaveBeenCalledTimes(1)
    expect(panelText(root)).not.toContain('still reading')
  })

  it('Write tags gives up the same way, keeping the list for another press', async () => {
    const fake = fakeState()
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    fake.settled.mockResolvedValueOnce(false)

    await press(root, 'Write tags')

    expect(panelText(root)).toContain('Cmdr is still reading the files. Try again in a moment.')
    expect(fake.apply).not.toHaveBeenCalled()
    expect(buttonNamed(root, 'Write tags').disabled).toBe(false)
  })

  it('a list that could not be made says so inside the panel', async () => {
    const fake = fakeState()
    fake.changes.mockResolvedValueOnce({ ok: false, error: { kind: 'taskFailed' } } as never)
    const { root } = await mountPanel(fake)

    await press(root, 'Write tags…')

    expect(panelText(root)).toContain('Cmdr couldn’t check what will change. Try again in a moment.')
    expect(buttonNamed(root, 'Write tags…').disabled).toBe(false)
  })

  it('a refused write says no file was written, and why, inside the panel', async () => {
    const fake = fakeState()
    fake.apply.mockResolvedValueOnce({ ok: false, error: { kind: 'notOpen' } } as never)
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')

    await press(root, 'Write tags')

    expect(panelText(root)).toContain('No file was written.')
    expect(panelText(root)).toContain('Close Multi-Rename and open it again.')
  })

  it('a write where every file was refused says so first, with each file’s reason', async () => {
    const fake = fakeState(PANEL_VIEW, [
      applyReport({
        applyId: null,
        failed: [
          { path: '/b/01.mp3', error: { type: 'badDate', data: '2010-20-09' } },
          { path: '/b/02.mp3', error: { type: 'write', data: 'x' } },
        ],
      }),
    ])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')

    await press(root, 'Write tags')

    expect(panelText(root)).toContain('No file was written.')
    expect(panelText(root)).toContain('01.mp3: its date “2010-20-09” isn’t a real date; correct it in the Date row')
    expect(panelText(root)).toContain('02.mp3: the tags couldn’t be saved')
  })

  it('with nothing edited, Write tags… still answers: nothing to write', async () => {
    const fake = fakeState({ ...PANEL_VIEW, dirty: false })
    fake.changes.mockResolvedValueOnce({ ok: true, value: [] })
    const { root } = await mountPanel(fake)

    expect(buttonNamed(root, 'Write tags…').disabled).toBe(false)
    await press(root, 'Write tags…')

    expect(panelText(root)).toContain('Nothing to write')
  })

  it('a read-only session disables Write tags…; the tab says why, once, above (TagsContent’s test)', async () => {
    const { root, fake } = await mountPanel(fakeState({ ...PANEL_VIEW, editable: false }))

    expect(buttonNamed(root, 'Write tags…').disabled).toBe(true)
    expect(panelText(root)).not.toContain('Tags can only be edited')
    expect(fake.changes).not.toHaveBeenCalled()
  })

  it.each([
    [{ kind: 'readOnly' }, 'Tags can only be edited for files on a local disk.'],
    [
      { kind: 'undo', error: { type: 'io', data: ['PermissionDenied', 'x'] } },
      'Couldn’t use the store that keeps the old tags for Undo.',
    ],
  ])('a write refused before any file (%j) says no file was written and why', async (error, reason) => {
    const fake = fakeState()
    fake.apply.mockResolvedValueOnce({ ok: false, error } as never)
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')

    await press(root, 'Write tags')

    expect(panelText(root)).toContain('No file was written.')
    expect(panelText(root)).toContain(reason)
  })

  it('a write that died partway says it stopped, without claiming no file was written', async () => {
    const fake = fakeState()
    fake.apply.mockResolvedValueOnce({ ok: false, error: { kind: 'taskFailed' } } as never)
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')

    await press(root, 'Write tags')

    expect(panelText(root)).toContain('The write stopped before it finished')
    expect(panelText(root)).not.toContain('No file was written.')
  })

  it('a refused Write anyway keeps what the first apply wrote and says the write stopped', async () => {
    const fake = fakeState(PANEL_VIEW, [
      applyReport({ written: ['/b/02.mp3'], wouldDrop: [{ path: '/b/01.mp3', items: ['TSOP'] }] }),
    ])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    fake.apply.mockResolvedValueOnce({ ok: false, error: { kind: 'notOpen' } } as never)

    await press(root, 'Write anyway')

    expect(panelText(root)).toContain('Tags written to 1 file')
    expect(panelText(root)).toContain('The write stopped before it finished')
  })

  it('Write tags pressed twice before the first press settles writes once', async () => {
    const fake = fakeState()
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')

    buttonNamed(root, 'Write tags').click()
    buttonNamed(root, 'Write tags').click()
    await settle()

    expect(fake.apply).toHaveBeenCalledTimes(1)
  })

  it('Back, Write anyway and Skip these files each answer', async () => {
    const wouldDrop = () => applyReport({ applyId: null, wouldDrop: [{ path: '/b/01.mp3', items: ['TSOP'] }] })
    const fake = fakeState(PANEL_VIEW, [wouldDrop(), applyReport({ written: ['/b/01.mp3'] }), wouldDrop()])
    const { root } = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Back')
    expect(panelText(root)).not.toContain('will change')

    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(panelText(root)).toContain('01.mp3: TSOP')
    await press(root, 'Write anyway')
    expect(panelText(root)).toContain('Tags written to 1 file')

    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    await press(root, 'Skip these files')
    expect(panelText(root)).toContain('Skipped, not written:')
    expect(root.querySelector('[role="alert"]')).toBeNull()
    expect(fake.apply).toHaveBeenCalledTimes(3)
  })
})
