/**
 * "Write anyway", shared by the Write dialog and ⌘⏎'s would-drop choice: it waits for the calls out, writes nothing
 * when an edit landed since the choice was asked, and otherwise sends the would-drop files alone, continuing the press.
 */
import { describe, expect, it, vi } from 'vitest'
import { applyReport, asState, fakeState, PANEL_VIEW } from './apply-panel.test-helper'
import { answerWriteAnyway } from './write-anyway'

const first = applyReport({ written: ['/b/02.mp3'], wouldDrop: [{ path: '/b/01.mp3', items: ['TSOP'] }] })
const choice = { reports: [first], kind: 'audiobook' as const, seen: PANEL_VIEW }

describe('answerWriteAnyway', () => {
  it('writes the would-drop files alone, after the player stops, continuing the press', async () => {
    const state = fakeState(PANEL_VIEW, [applyReport({ applyId: 'A2', written: ['/b/01.mp3'] })])
    const onWriting = vi.fn()
    const answer = await answerWriteAnyway(asState(state), choice, onWriting)
    expect(state.apply).toHaveBeenCalledWith(
      { kind: 'audiobook', allowFrameLoss: true, skip: ['/b/02.mp3', '/b/03.mp3'] },
      { after: [first] },
    )
    expect(onWriting.mock.invocationCallOrder[0]).toBeLessThan(state.apply.mock.invocationCallOrder[0])
    expect(answer).toEqual({
      kind: 'written',
      reports: [first, applyReport({ applyId: 'A2', written: ['/b/01.mp3'] })],
    })
  })

  it('an edit since the choice writes nothing: the changes are listed again', async () => {
    const state = fakeState({ ...PANEL_VIEW })
    expect(await answerWriteAnyway(asState(state), choice)).toEqual({ kind: 'changed' })
    expect(state.apply).not.toHaveBeenCalled()
  })

  it('a call still out writes nothing', async () => {
    const state = fakeState(PANEL_VIEW)
    state.settled.mockResolvedValue(false)
    expect(await answerWriteAnyway(asState(state), choice)).toEqual({ kind: 'stillBusy' })
    expect(state.apply).not.toHaveBeenCalled()
  })

  it('a refusal comes back typed', async () => {
    const state = fakeState(PANEL_VIEW)
    state.apply.mockResolvedValue({ ok: false, error: { kind: 'readOnly' } } as never)
    expect(await answerWriteAnyway(asState(state), choice)).toEqual({ kind: 'refused', error: { kind: 'readOnly' } })
  })
})
