/**
 * The Write dialog with Audiobookshelf's side files on: the list names `reader.txt` and `desc.txt` under the files,
 * its write may replace only the side files it showed (with the old and new text it showed), side files ride along
 * with a tag write and never go alone, one held back at the write is listed as not written, and the write sends the
 * setting as the list read it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SideFileChange } from '#lib/tauri-commands/index.js'

const settings = vi.hoisted(() => ({ absSideFiles: true }))
vi.mock('#lib/settings/index.js', () => ({
  getSetting: (id: string) => (id === 'multiRename.tagEditor.absSideFiles' ? settings.absSideFiles : false),
}))

import ApplyPanel from './ApplyPanel.svelte'
import { mountInto } from './tag-cell.test-helper'
import { applyReport, asState, buttonNamed, fakeState, press } from './apply-panel.test-helper'

let cleanup: (() => void) | undefined
beforeEach(() => {
  settings.absSideFiles = true
})
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
        allowSideFiles: [{ path: '/b/reader.txt', old: 'Someone Else\n', text: 'Vladimir Kudla' }],
        sideFiles: true,
      },
      { after: [] },
    )
  })

  it('side files never go alone: with no file to change, nothing is listed or written', async () => {
    const fake = fakeState()
    fake.changes.mockResolvedValue({ ok: true, value: [] })
    fake.sideFileChanges.mockResolvedValue({ ok: true, value: SIDE })
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(fake.sideFileChanges).not.toHaveBeenCalled()
    expect(buttonNamed(root, 'Write tags').disabled).toBe(true)
  })

  it('a withheld side file says why', async () => {
    const fake = fakeState()
    fake.sideFileChanges.mockResolvedValue({
      ok: true,
      value: [{ kind: 'withheld', file: 'reader', reason: 'tracksDisagree' }],
    })
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    expect(listed(root, 'Beside the book:')).toEqual(['reader.txt isn’t written: the tracks name different narrators'])
  })

  it('one held back at the write is listed as not written', async () => {
    const held = applyReport({
      written: ['/b/01.mp3', '/b/02.mp3'],
      sideFilesWritten: ['/b/desc.txt'],
      sideFilesHeld: ['/b/reader.txt'],
    })
    const fake = fakeState(undefined, [held])
    fake.sideFileChanges.mockResolvedValue({ ok: true, value: SIDE })
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    expect(fake.changes).toHaveBeenCalledTimes(1)
    expect(root.textContent).toContain('Also written beside the book: desc.txt')
    expect(listed(root, 'Not written beside the book:')).toEqual(['reader.txt'])
  })

  it('the write sends the setting as the list read it, even if it flips meanwhile', async () => {
    settings.absSideFiles = false
    const fake = fakeState()
    const root = await mountPanel(fake)
    await press(root, 'Write tags…')
    settings.absSideFiles = true
    await press(root, 'Write tags')
    expect(fake.sideFileChanges).not.toHaveBeenCalled()
    expect(fake.apply).toHaveBeenCalledWith(expect.objectContaining({ sideFiles: false }), { after: [] })
  })
})
