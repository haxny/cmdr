/**
 * The name mask's syntax in the sheet's state: a pasted foobar2000 pattern switches the mask to foobar
 * (and back on request, keeping the text), and a mask that doesn't parse, in either syntax, shows no new
 * names: every row keeps its old one until the mask is fixed, and Rename and Results don't run.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const { ipc } = vi.hoisted(() => ({
  ipc: {
    previewMultiRename: vi.fn(),
    getMultiRenamePreviewRows: vi.fn(),
    applyMultiRename: vi.fn(),
    writeMultiRenameNames: vi.fn(),
    getMultiRenamePresets: vi.fn(),
    getMultiRenameLastSettings: vi.fn(),
    saveMultiRenameLastSettings: vi.fn(),
    getMultiRenameMediaProgress: vi.fn(),
    onMultiRenameMediaProgress: vi.fn(),
    prioritizeMultiRenameMedia: vi.fn(),
  },
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)

import { PREVIEW_DELAY_MS, createMultiRenameState } from './multi-rename-state.svelte'
import { DEFAULT_SPEC } from './spec'

/** The user's pattern from the screenshot that showed cut names. */
const READ_BY =
  "[%album artist% - ]'('%year%')' %album% '('cte %performer%, %publisher%')' - %disc%$num(%tracknumber%,2)"

const row = (index: number, oldName: string, newName: string, type = 'ready') => ({
  row: index,
  oldName,
  newName,
  status: { type },
})

function preview(previewId: number, rows: ReturnType<typeof row>[]) {
  const ready = rows.filter((r) => r.status.type === 'ready').length
  return {
    ok: true,
    value: { previewId, counts: { ready, unchanged: rows.length - ready, problems: 0, reading: 0 }, rows },
  }
}

const unknownPlaceholder = {
  ok: false,
  error: { type: 'spec', error: { type: 'nameMask', error: { type: 'unknown', placeholder: '=tag.performer' } } },
}
const unknownField = {
  ok: false,
  error: { type: 'spec', error: { type: 'pattern', error: { type: 'unknownField', name: 'nope', at: 0 } } },
}

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) await Promise.resolve()
}

describe('the name mask syntax', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    ipc.previewMultiRename.mockResolvedValue(preview(1, [row(0, 'Denik 03.mp3', 'Anne Frank - Denik - 03.mp3')]))
    ipc.getMultiRenamePresets.mockResolvedValue([])
    ipc.getMultiRenameLastSettings.mockResolvedValue(null)
    ipc.getMultiRenameMediaProgress.mockResolvedValue({ ok: true, value: { read: 0, total: 0, done: true } })
    ipc.onMultiRenameMediaProgress.mockResolvedValue(() => {})
    ipc.prioritizeMultiRenameMedia.mockResolvedValue(undefined)
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('switches to foobar when the user pastes a foobar pattern, and says so', async () => {
    const tool = createMultiRenameState('S')
    await settle()
    expect(tool.spec.maskSyntax).toBe('cmdr')
    tool.update({ nameMask: READ_BY })
    expect(tool.spec.maskSyntax).toBe('foobar')
    expect(tool.syntaxSwitched).toBe(true)
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(ipc.previewMultiRename.mock.lastCall?.[1]).toMatchObject({ nameMask: READ_BY, maskSyntax: 'foobar' })
    tool.dispose()
  })

  it('switches back to Cmdr on request, keeping the text, and typing on stays Cmdr', async () => {
    const tool = createMultiRenameState('S')
    await settle()
    tool.update({ nameMask: READ_BY })
    tool.undoSyntaxSwitch()
    expect(tool.spec).toMatchObject({ nameMask: READ_BY, maskSyntax: 'cmdr' })
    expect(tool.syntaxSwitched).toBe(false)
    tool.update({ nameMask: `${READ_BY} x` })
    expect(tool.spec.maskSyntax).toBe('cmdr')
    tool.dispose()
  })

  it('leaves a Cmdr mask with plain percent signs in Cmdr', async () => {
    const tool = createMultiRenameState('S')
    await settle()
    tool.update({ nameMask: '[N] 100%' })
    expect(tool.spec.maskSyntax).toBe('cmdr')
    expect(tool.syntaxSwitched).toBe(false)
    tool.dispose()
  })

  it('a chosen syntax is the user’s: no notice, and the text stays', async () => {
    const tool = createMultiRenameState('S')
    await settle()
    tool.update({ nameMask: READ_BY })
    tool.update({ maskSyntax: 'cmdr' })
    expect(tool.syntaxSwitched).toBe(false)
    tool.update({ maskSyntax: 'foobar' })
    expect(tool.spec).toMatchObject({ nameMask: READ_BY, maskSyntax: 'foobar' })
    expect(tool.syntaxSwitched).toBe(false)
    tool.dispose()
  })

  it('reads tags for a foobar tag field', async () => {
    const tool = createMultiRenameState('S')
    await settle()
    tool.update({ nameMask: '%filename%' })
    expect(tool.usesMedia).toBe(false)
    tool.update({ nameMask: '%title%' })
    expect(tool.usesMedia).toBe(true)
    tool.dispose()
  })

  for (const [syntax, answer, mask] of [
    ['Cmdr', unknownPlaceholder, '(cte [=tag.performer], x)'],
    ['foobar', unknownField, '%nope%'],
  ] as const) {
    it(`an invalid ${syntax} mask shows no new names, and Rename and Results don't run`, async () => {
      const tool = createMultiRenameState('S')
      await settle()
      expect(tool.rowAt(0)?.newName).toBe('Anne Frank - Denik - 03.mp3')
      ipc.previewMultiRename.mockResolvedValue(answer)
      tool.update({ nameMask: mask })
      await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
      expect(tool.maskInvalid).toBe(true)
      expect(tool.rowAt(0)).toMatchObject({ oldName: 'Denik 03.mp3', newName: 'Denik 03.mp3' })
      expect(tool.source.getRow(0)?.newName).toBe('Denik 03.mp3')
      expect(await tool.apply()).toBeNull()
      expect(await tool.writeNames()).toBeNull()
      expect(ipc.applyMultiRename).not.toHaveBeenCalled()
      expect(ipc.writeMultiRenameNames).not.toHaveBeenCalled()
      tool.dispose()
    })
  }

  it('a sheet that opens on an invalid mask still lists its files under their old names', async () => {
    ipc.getMultiRenameLastSettings.mockResolvedValue({ spec: { ...DEFAULT_SPEC, nameMask: '[X]' }, presetId: null })
    ipc.previewMultiRename.mockImplementation((_s: string, spec: { nameMask: string }) =>
      Promise.resolve(
        spec.nameMask === '[X]' ? unknownPlaceholder : preview(4, [row(0, 'a.mp3', 'a.mp3', 'unchanged')]),
      ),
    )
    const tool = createMultiRenameState('S')
    await settle()
    await settle()
    expect(tool.maskInvalid).toBe(true)
    expect(tool.total).toBe(1)
    expect(tool.rowAt(0)).toMatchObject({ oldName: 'a.mp3', newName: 'a.mp3' })
    expect(tool.spec.nameMask).toBe('[X]')
    tool.dispose()
  })
})
