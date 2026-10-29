/**
 * The sheet's name mask syntax: the "Cmdr / foobar2000" switch beside the name mask, the notice when a
 * pasted foobar2000 pattern switched it on its own (with its one-click way back), foobar field inserts
 * in place of Cmdr's placeholder chips, and a foobar error worded with its place and underlined.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'

const ipc = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  getMultiRenamePreviewRows: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getMultiRenameMediaProgress: vi.fn(() => Promise.resolve({ ok: true, value: { read: 0, total: 0, done: true } })),
  onMultiRenameMediaProgress: vi.fn(() => Promise.resolve(() => {})),
  prioritizeMultiRenameMedia: vi.fn(() => Promise.resolve()),
  renderMultiRenameExamples: vi.fn(() => Promise.resolve([])),
  getMultiRenamePresets: vi.fn(() => Promise.resolve([])),
  getMultiRenameLastSettings: vi.fn(() => Promise.resolve(null)),
  saveMultiRenameLastSettings: vi.fn(() => Promise.resolve()),
  getMultiRenameHistory: vi.fn(() => Promise.resolve([])),
  ...ipc,
}))

/** The user's pattern from the screenshot that showed cut names. */
const READ_BY = "[%album artist% - ]'('%year%')' %album% '('cte %performer%')' - %disc%$num(%tracknumber%,2)"

const READY = {
  previewId: 1,
  counts: { ready: 1, unchanged: 0, problems: 0, reading: 0 },
  rows: [{ row: 0, oldName: '03.mp3', newName: 'Anne Frank - (1947) Denik - 03.mp3', status: { type: 'ready' } }],
}

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

async function mountSheet(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MultiRenameDialog, {
    target,
    props: {
      session: { sessionId: 'S', count: 1 },
      nothingSelected: false,
      onApplied: vi.fn(),
      onUndoStarted: vi.fn(),
      onClose: vi.fn(),
    },
  })
  await settle()
  return target
}

async function type(root: HTMLElement, text: string): Promise<void> {
  const mask = root.querySelector<HTMLInputElement>('input')
  if (!mask) throw new Error('no name mask')
  mask.value = text
  mask.dispatchEvent(new Event('input', { bubbles: true }))
  await new Promise((resolve) => setTimeout(resolve, 150))
  await settle()
}

function syntaxButton(root: HTMLElement, label: string): HTMLButtonElement {
  const group = root.querySelector('[aria-label="Name mask syntax"]')
  const button = [...(group?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find(
    (b) => b.textContent.trim() === label,
  )
  if (!button) throw new Error(`no ${label} option`)
  return button
}

const pressed = (button: HTMLButtonElement): boolean =>
  button.getAttribute('aria-pressed') === 'true' || button.getAttribute('data-state') === 'on'

describe('the name mask syntax switch', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: READY })
  })

  it('starts on Cmdr, with Cmdr placeholder chips', async () => {
    const root = await mountSheet()
    expect(pressed(syntaxButton(root, 'Cmdr'))).toBe(true)
    expect(root.querySelector('.placeholders')?.textContent).toContain('[N]')
  })

  it('a pasted foobar pattern switches to foobar2000, says so, and one click goes back keeping the text', async () => {
    const root = await mountSheet()
    await type(root, READ_BY)
    expect(pressed(syntaxButton(root, 'foobar2000'))).toBe(true)
    expect(ipc.previewMultiRename.mock.lastCall?.[1]).toMatchObject({ nameMask: READ_BY, maskSyntax: 'foobar' })
    const notice = root.querySelector('.results-notice')
    expect(notice?.textContent).toContain('foobar2000 pattern')
    expect(root.querySelector('.placeholders')?.textContent).toContain('%album%')
    expect(root.querySelector('.placeholders')?.textContent).not.toContain('[N]')
    expect(root.querySelector('.mask-input')?.querySelector('.token-marker')).toBeNull()

    const back = [...(notice?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) =>
      b.textContent.includes('Use Cmdr syntax'),
    )
    back?.click()
    await settle()
    expect(pressed(syntaxButton(root, 'Cmdr'))).toBe(true)
    expect(root.querySelector<HTMLInputElement>('input')?.value).toBe(READ_BY)
    expect(root.querySelector('.results-notice')?.textContent).not.toContain('foobar2000 pattern')
  })

  it('a foobar field insert goes into the name mask', async () => {
    const root = await mountSheet()
    syntaxButton(root, 'foobar2000').click()
    await settle()
    const album = [...root.querySelectorAll<HTMLButtonElement>('.placeholders button')].find(
      (b) => b.textContent.trim() === '%album%',
    )
    album?.click()
    await settle()
    expect(root.querySelector<HTMLInputElement>('input')?.value).toContain('%album%')
  })

  it('a foobar error says what and where, underlines it, and shows no new names', async () => {
    const root = await mountSheet()
    ipc.previewMultiRename.mockResolvedValue({
      ok: false,
      error: { type: 'spec', error: { type: 'pattern', error: { type: 'unknownField', name: 'nope', at: 7 } } },
    })
    await type(root, 'Deník %nope%')
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBe(
      'Cmdr doesn’t know the field %nope% (character 7)',
    )
    expect(root.querySelector('.error-mark')).not.toBeNull()
    expect(root.querySelector('.preview-new-name')?.textContent).not.toContain('Anne Frank')
  })
})
