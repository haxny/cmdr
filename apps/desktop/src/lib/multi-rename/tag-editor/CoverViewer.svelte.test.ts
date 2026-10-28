/** The large view of one cover: the picture, what it is and on how many files, "Use for all files", Close. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { CoverChoice } from '#lib/ipc/bindings.js'
import CoverViewer from './CoverViewer.svelte'
import { mountInto } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))
vi.mock('#lib/tauri-commands/index.js', async (original) => ({
  ...(await original<object>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

const FRONT: CoverChoice = { key: 'k1', mime: 'image/jpeg', bytes: 2048, width: 600, height: 400, files: 12 }
const URL = 'data:image/jpeg;base64,/9j/'

async function mountViewer(
  over: { choice?: CoverChoice; total?: number; editable?: boolean; image?: string | null } = {},
) {
  const props = {
    choice: over.choice ?? FRONT,
    total: over.total ?? 22,
    image: over.image === undefined ? URL : over.image,
    editable: over.editable ?? true,
    onUseForAll: vi.fn(),
    onClose: vi.fn(),
  }
  const mounted = await mountInto(CoverViewer, props)
  cleanup = mounted.cleanup
  await tick()
  return props
}

function inDialog(text: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(
    (b) => b.textContent.trim() === text,
  )
  if (!found) throw new Error(`no ${text} in the dialog`)
  return found
}

describe('CoverViewer', () => {
  it('shows the cover large with what it is and on how many files', async () => {
    await mountViewer()
    const dialog = document.querySelector('[role="dialog"]')
    expect(dialog?.querySelector('img')?.getAttribute('src')).toBe(URL)
    expect(dialog?.textContent).toContain('image/jpeg, 600×400 px, 2.00 KiB')
    expect(dialog?.textContent).toContain('On 12 of 22 files')
  })

  it('"Use for all files" gives every file this cover', async () => {
    const { onUseForAll } = await mountViewer()
    inDialog('Use for all files').click()
    expect(onUseForAll).toHaveBeenCalledWith('k1')
  })

  it('"Use for all files" is off when every file has it already, or read-only', async () => {
    await mountViewer({ total: 12 })
    expect(inDialog('Use for all files').disabled).toBe(true)
    cleanup?.()
    document.body.innerHTML = ''
    await mountViewer({ editable: false })
    expect(inDialog('Use for all files').disabled).toBe(true)
  })

  it('says so when the picture can’t be shown', async () => {
    await mountViewer({ image: null })
    expect(document.querySelector('[role="dialog"] img')).toBeNull()
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('Can’t show this picture')
  })

  it('Close closes', async () => {
    const { onClose } = await mountViewer()
    inDialog('Close').click()
    expect(onClose).toHaveBeenCalled()
  })
})
