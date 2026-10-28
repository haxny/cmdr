/**
 * The Write dialog with Audiobookshelf's side files on: the list names `reader.txt` and `desc.txt` under the files,
 * its write may replace only the side files it showed (with the old text it showed), side files alone are a write,
 * and one held back at the write (it changed since the list) lists again.
 */
import { afterEach, describe, expect, it } from 'vitest'
import type { SideFileChange } from '#lib/tauri-commands/index.js'
import ApplyPanel from './ApplyPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { applyReport, asState, buttonNamed, fakeState, press } from './apply-panel.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountPanel(fake: ReturnType<typeof fakeState>) {
  const mounted = await mountInto(ApplyPanel, { tags: asState(fake) })
  cleanup = mounted.cleanup
  return mounted.root
}

const listed = (root: HTMLElement, label: string) =>
  [...(root.querySelector(`[aria-label="${label}"]`)?.querySelectorAll('li') ?? [])].map((li) => li.textContent.trim())

const SIDE: SideFileChange[] = [
  { kind: 'replace', file: 'reader', path: '/b/reader.txt', old: 'Someone Else\n', text: 'Vladimir Kudla' },
  { kind: 'create', file: 'desc', path: '/b/desc.txt', text: 'A book about a dog.' },
]

describe('ApplyPanel side files', () => {
  it('lists them under the files, and its write may replace only the one it showed', async () => {
    const fake = fakeState()
    fake.sideFileChanges.mockResolvedValue({ ok: true, value: SIDE })
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(listed(root, 'Beside the book:')).toEqual([
      'Replace reader.txt: “Someone Else” becomes “Vladimir Kudla”',
      'Create desc.txt: A book about a dog.',
    ])
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenCalledWith(
      {
        kind: 'audiobook',
        allowFrameLoss: false,
        skip: [],
        allowRemovals: [],
        allowSideFiles: [{ path: '/b/reader.txt', old: 'Someone Else\n' }],
      },
      { after: [] },
    )
  })

  it('side files alone are a write', async () => {
    const fake = fakeState(undefined, [applyReport({ sideFilesWritten: ['/b/desc.txt'] })])
    fake.changes.mockResolvedValue({ ok: true, value: [] })
    fake.sideFileChanges.mockResolvedValue({ ok: true, value: [SIDE[1]] })
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(buttonNamed(root, 'Write tags').disabled).toBe(false)
    await press(root, 'Write tags')
    expect(fake.apply).toHaveBeenCalledTimes(1)
    expect(root.textContent).toContain('Also written beside the book: desc.txt')
  })

  it('a withheld side file says why, and alone writes nothing', async () => {
    const fake = fakeState()
    fake.changes.mockResolvedValue({ ok: true, value: [] })
    fake.sideFileChanges.mockResolvedValue({
      ok: true,
      value: [{ kind: 'withheld', file: 'reader', reason: 'tracksDisagree' }],
    })
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(listed(root, 'Beside the book:')).toEqual(['reader.txt isn’t written: the tracks name different narrators'])
    expect(buttonNamed(root, 'Write tags').disabled).toBe(true)
  })

  it('one held back at the write lists again, continuing the press', async () => {
    const held = applyReport({ written: ['/b/01.mp3'], sideFilesHeld: ['/b/reader.txt'] })
    const fake = fakeState(undefined, [held])
    fake.sideFileChanges.mockResolvedValue({ ok: true, value: [SIDE[1]] })
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(fake.changes).toHaveBeenCalledTimes(2)
    expect(fake.sideFileChanges).toHaveBeenCalledTimes(2)
    expect(root.textContent).toContain('Something changed since the list was made, so here it is again.')
  })
})
