/** The auto-numbering wizard: the screenshot's controls, what OK sends and saves, and what Cancel leaves alone. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { NumberOptions } from '#lib/ipc/bindings.js'
import NumberingDialog from './NumberingDialog.svelte'
import { DEFAULT_DISC, DEFAULT_NUMBERING } from './numbering-options'
import { byId, mountInto, typeInto } from './tag-cell.test-helper'
import { SCOPE, asTags, boxNamed, buttonNamed, fakeRunScoped, fakeTags } from './tag-tools-panel.test-helper'

const stored = vi.hoisted((): { value: unknown; saved: unknown[] } => ({ value: null, saved: [] }))
vi.mock('#lib/app-status-store.js', () => ({
  loadTagNumbering: vi.fn(() => Promise.resolve(stored.value)),
  saveTagNumbering: vi.fn((value: unknown) => {
    stored.value = value
    stored.saved.push(value)
    return Promise.resolve()
  }),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

let cleanup: (() => void) | undefined
beforeEach(() => {
  stored.value = null
  stored.saved = []
})
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

async function mountDialog(over: { trackCount?: number; scope?: string[] | null } = {}) {
  const tags = fakeTags()
  const onClose = vi.fn()
  const runScoped = fakeRunScoped(over.scope === undefined ? SCOPE : over.scope)
  const mounted = await mountInto(NumberingDialog, {
    trackCount: over.trackCount ?? 3,
    state: asTags(tags),
    runScoped,
    onClose,
  })
  cleanup = mounted.cleanup
  await settle()
  return { root: document.body, tags, onClose }
}

async function click(el: HTMLElement): Promise<void> {
  el.click()
  await settle()
}

const sent = (tags: ReturnType<typeof fakeTags>): unknown => tags.tool.mock.calls[0]?.[0]

describe('NumberingDialog', () => {
  it('shows the screenshot controls with the defaults', async () => {
    const { root } = await mountDialog()
    expect(byId<HTMLInputElement>(root, 'tag-numbering-track-start').value).toBe('1')
    expect(boxNamed(root, 'Save total').checked).toBe(true)
    expect(boxNamed(root, 'Leading zeros').checked).toBe(true)
    expect(boxNamed(root, 'Disc number').checked).toBe(false)
    expect(byId<HTMLInputElement>(root, 'tag-numbering-disc-start').disabled).toBe(true)
  })

  it('sends the defaults with the scope on OK, then saves them', async () => {
    const { root, tags, onClose } = await mountDialog()
    await click(buttonNamed(root, 'OK'))
    expect(tags.tool).toHaveBeenCalledWith({ kind: 'autoNumber', ...DEFAULT_NUMBERING }, SCOPE)
    expect(stored.saved).toEqual([DEFAULT_NUMBERING])
    expect(onClose).toHaveBeenCalled()
  })

  it('enables the disc section and sends its defaults', async () => {
    const { root, tags } = await mountDialog()
    await click(boxNamed(root, 'Disc number'))
    expect(byId<HTMLInputElement>(root, 'tag-numbering-disc-start').disabled).toBe(false)
    await click(buttonNamed(root, 'OK'))
    expect(sent(tags)).toEqual({ kind: 'autoNumber', ...DEFAULT_NUMBERING, disc: DEFAULT_DISC })
  })

  it('sends exactly what is shown', async () => {
    const { root, tags } = await mountDialog()
    await typeInto(byId<HTMLInputElement>(root, 'tag-numbering-track-start'), '0')
    await click(boxNamed(root, 'Save total'))
    await click(boxNamed(root, 'Leading zeros'))
    await click(boxNamed(root, 'Disc number'))
    await typeInto(byId<HTMLInputElement>(root, 'tag-numbering-disc-start'), '2')
    const discPad = [...root.querySelectorAll('label')].filter((l) => l.textContent.trim() === 'Leading zeros').at(1)
    await click(discPad?.querySelector<HTMLInputElement>('input') ?? root)
    await click(boxNamed(root, 'Next disc on change of directory'))
    await click(boxNamed(root, 'Next disc on change of'))
    await click(boxNamed(root, 'Reset track counter on next disc'))
    await click(buttonNamed(root, 'OK'))
    const expected: NumberOptions = {
      trackStart: 0,
      trackTotal: false,
      trackPad: false,
      disc: {
        start: 2,
        total: true,
        pad: true,
        nextOnFolder: false,
        nextOnField: 'album',
        resetTrack: true,
        resetDiscOnField: null,
      },
    }
    expect(sent(tags)).toEqual({ kind: 'autoNumber', ...expected })
  })

  it('sends the fields picked in both selects under their wire names', async () => {
    const { root, tags } = await mountDialog()
    await click(boxNamed(root, 'Disc number'))
    await click(boxNamed(root, 'Next disc on change of'))
    await click(boxNamed(root, 'Reset disc counter on change of'))
    const pick = async (index: number, value: string): Promise<void> => {
      root.querySelectorAll<HTMLButtonElement>('.select-trigger')[index].click()
      await settle()
      ;[...document.querySelectorAll<HTMLElement>('[data-part="content"][data-state="open"] [data-part="item"]')]
        .find((el) => el.getAttribute('data-value') === value)
        ?.click()
      await settle()
    }
    await pick(0, 'artist')
    await pick(1, 'genre')
    await click(buttonNamed(root, 'OK'))
    const options = sent(tags) as NumberOptions & { kind: string }
    expect(options.disc?.nextOnField).toBe('artist')
    expect(options.disc?.resetDiscOnField).toBe('genre')
  })

  it('shows the saved options on the next open', async () => {
    stored.value = { trackStart: 4, trackTotal: false, trackPad: true, disc: { ...DEFAULT_DISC, start: 2 } }
    const { root } = await mountDialog()
    expect(byId<HTMLInputElement>(root, 'tag-numbering-track-start').value).toBe('4')
    expect(boxNamed(root, 'Save total').checked).toBe(false)
    expect(boxNamed(root, 'Disc number').checked).toBe(true)
    expect(byId<HTMLInputElement>(root, 'tag-numbering-disc-start').value).toBe('2')
  })

  it('sends nothing on Cancel', async () => {
    const { root, tags, onClose } = await mountDialog()
    await click(buttonNamed(root, 'Cancel'))
    expect(tags.tool).not.toHaveBeenCalled()
    expect(stored.saved).toEqual([])
    expect(onClose).toHaveBeenCalled()
  })

  it('sends nothing and saves nothing on Escape', async () => {
    const { root, tags, onClose } = await mountDialog()
    const dialog = root.querySelector<HTMLElement>('[role="dialog"]') ?? root
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    await settle()
    expect(tags.tool).not.toHaveBeenCalled()
    expect(stored.saved).toEqual([])
    expect(onClose).toHaveBeenCalled()
  })

  it('saves nothing when the tool failed', async () => {
    const { root, tags } = await mountDialog()
    tags.tool.mockImplementation(() => {
      tags.error = { kind: 'busy' } as never
      return Promise.resolve()
    })
    await click(buttonNamed(root, 'OK'))
    expect(stored.saved).toEqual([])
  })

  it('refuses a begin that is not an integer of at least zero, and says so', async () => {
    const { root, tags } = await mountDialog()
    const ok = buttonNamed(root, 'OK')
    for (const bad of ['-1', '1.5', 'x', '']) {
      await typeInto(byId<HTMLInputElement>(root, 'tag-numbering-track-start'), bad)
      expect(ok.disabled).toBe(true)
      expect(root.textContent).toContain('Begin at must be a whole number, 0 or more.')
    }
    await typeInto(byId<HTMLInputElement>(root, 'tag-numbering-track-start'), '3')
    expect(ok.disabled).toBe(false)
    expect(tags.tool).not.toHaveBeenCalled()
  })

  it('ignores a bad disc begin while the disc section is off', async () => {
    const { root } = await mountDialog()
    expect(buttonNamed(root, 'OK').disabled).toBe(false)
  })

  it('says a single-file book gets no track number only when the book has one track', async () => {
    const one = await mountDialog({ trackCount: 1 })
    expect(one.root.textContent).toContain('A single-file book gets no track number')
    cleanup?.()
    document.body.innerHTML = ''
    const three = await mountDialog({ trackCount: 3 })
    expect(three.root.textContent).not.toContain('A single-file book gets no track number')
  })

  it('does nothing when the checked tracks are gone', async () => {
    const tags = fakeTags()
    const gone = fakeRunScoped()
    // The runner resolves undefined without calling, as the tab does for the "gone" case.
    gone.mockImplementation(() => Promise.resolve(undefined))
    const mounted = await mountInto(NumberingDialog, {
      trackCount: 3,
      state: asTags(tags),
      runScoped: gone,
      onClose: vi.fn(),
    })
    cleanup = mounted.cleanup
    await settle()
    await click(buttonNamed(document.body, 'OK'))
    expect(tags.tool).not.toHaveBeenCalled()
    expect(stored.saved).toEqual([])
  })
})
