/** The would-drop choice: it lists each file and what it would lose, and each button answers once. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import WouldDropDialog from './WouldDropDialog.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const ROWS = [
  { path: '/b/01 Prvni.mp3', items: ['TSOP', 'COMM:ces'] },
  { path: '/b/02.mp3', items: ['TITLE#2'] },
]

async function mountDialog(busy = false) {
  const onWriteAnyway = vi.fn()
  const onSkip = vi.fn()
  const mounted = await mountInto(WouldDropDialog, { rows: ROWS, busy, onWriteAnyway, onSkip })
  cleanup = mounted.cleanup
  return { root: mounted.root, onWriteAnyway, onSkip }
}

const buttonNamed = (root: HTMLElement, text: string) =>
  [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === text)

describe('WouldDropDialog', () => {
  it('lists every file with the items it would lose', async () => {
    const { root } = await mountDialog()
    const rows = [...root.querySelectorAll('li')].map((li) => li.textContent.trim())
    expect(rows).toEqual(['01 Prvni.mp3: TSOP, COMM:ces', '02.mp3: TITLE#2'])
    expect(root.textContent).toContain('Writing 2 files would drop the tag data listed below')
  })

  it('starts on the safe choice', async () => {
    const { root } = await mountDialog()
    expect(document.activeElement).toBe(buttonNamed(root, 'Skip these files'))
  })

  it('Write anyway and Skip these files each call their own answer', async () => {
    const { root, onWriteAnyway, onSkip } = await mountDialog()
    buttonNamed(root, 'Write anyway')?.click()
    expect(onWriteAnyway).toHaveBeenCalledTimes(1)
    expect(onSkip).not.toHaveBeenCalled()
    buttonNamed(root, 'Skip these files')?.click()
    expect(onSkip).toHaveBeenCalledTimes(1)
  })

  it('while the choice runs, both buttons rest', async () => {
    const { root } = await mountDialog(true)
    expect(buttonNamed(root, 'Write anyway')?.disabled).toBe(true)
    expect(buttonNamed(root, 'Skip these files')?.disabled).toBe(true)
  })
})
