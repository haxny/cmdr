/**
 * Tests for the Multi-Rename Tauri command wrappers: typed results pass through
 * as `{ ok, value }` / `{ ok, error }`; presets and the last settings are plain
 * pass-throughs, and Results writes and reads its names file.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    previewMultiRename: vi.fn(),
    applyMultiRename: vi.fn(),
    getMultiRenamePresets: vi.fn(),
    saveMultiRenamePreset: vi.fn(),
    deleteMultiRenamePreset: vi.fn(),
    writeMultiRenameNames: vi.fn(),
    readMultiRenameNames: vi.fn(),
    getMultiRenameLastSpec: vi.fn(),
    saveMultiRenameLastSpec: vi.fn(),
  },
}))

import { commands, type MultiRenameSpec } from '$lib/ipc/bindings'
import {
  applyMultiRename,
  deleteMultiRenamePreset,
  getMultiRenameLastSpec,
  getMultiRenamePresets,
  previewMultiRename,
  readMultiRenameNames,
  saveMultiRenameLastSpec,
  saveMultiRenamePreset,
  writeMultiRenameNames,
} from './multi-rename'

const spec = { nameMask: '[N]', extensionMask: '[E]' } as unknown as MultiRenameSpec
const edits = [{ oldName: 'a.txt', newName: 'b.txt' }]

describe('multi-rename wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('answers a preview with its rows, or with why there are none', async () => {
    const rows = [{ row: 0, oldName: 'a', newName: 'b', status: { type: 'ready' } }]
    vi.mocked(commands.previewMultiRename).mockResolvedValueOnce({ status: 'ok', data: rows } as never)
    expect(await previewMultiRename('L', false, null, spec, edits)).toEqual({ ok: true, value: rows })
    expect(commands.previewMultiRename).toHaveBeenCalledWith('L', false, null, spec, edits)

    vi.mocked(commands.previewMultiRename).mockResolvedValueOnce({ status: 'error', error: { type: 'gone' } } as never)
    expect(await previewMultiRename('L', false, [2, 0], spec, [])).toEqual({ ok: false, error: { type: 'gone' } })
  })

  it('starts a rename with what the user saw, and reports a refusal', async () => {
    const expected = [{ row: 0, oldName: 'a', newName: 'b' }]
    vi.mocked(commands.applyMultiRename).mockResolvedValueOnce({
      status: 'ok',
      data: { operationId: 'op', renaming: 1 },
    } as never)
    expect(await applyMultiRename('L', true, null, spec, edits, expected)).toEqual({
      ok: true,
      value: { operationId: 'op', renaming: 1 },
    })
    expect(commands.applyMultiRename).toHaveBeenCalledWith('L', true, null, spec, edits, expected)

    vi.mocked(commands.applyMultiRename).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'previewOutOfDate' },
    } as never)
    expect(await applyMultiRename('L', true, null, spec, [], expected)).toEqual({
      ok: false,
      error: { type: 'previewOutOfDate' },
    })
  })

  it('passes presets through', async () => {
    const preset = { id: 'p', name: 'Plain', spec }
    vi.mocked(commands.getMultiRenamePresets).mockResolvedValueOnce([preset] as never)
    expect(await getMultiRenamePresets()).toEqual([preset])
    await saveMultiRenamePreset(preset)
    expect(commands.saveMultiRenamePreset).toHaveBeenCalledWith(preset)
    await deleteMultiRenamePreset('p')
    expect(commands.deleteMultiRenamePreset).toHaveBeenCalledWith('p')
  })

  it('passes the last settings through', async () => {
    vi.mocked(commands.getMultiRenameLastSpec).mockResolvedValueOnce(spec)
    expect(await getMultiRenameLastSpec()).toEqual(spec)
    await saveMultiRenameLastSpec(spec)
    expect(commands.saveMultiRenameLastSpec).toHaveBeenCalledWith(spec)
  })

  it('writes the names file and reads the edits back, or says why not', async () => {
    vi.mocked(commands.writeMultiRenameNames).mockResolvedValueOnce({ status: 'ok', data: '/tmp/n.txt' } as never)
    expect(await writeMultiRenameNames('L', false, null, spec, [])).toEqual({ ok: true, value: '/tmp/n.txt' })
    expect(commands.writeMultiRenameNames).toHaveBeenCalledWith('L', false, null, spec, [])

    vi.mocked(commands.readMultiRenameNames).mockResolvedValueOnce({ status: 'ok', data: edits } as never)
    expect(await readMultiRenameNames()).toEqual({ ok: true, value: edits })
    vi.mocked(commands.readMultiRenameNames).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'noFile' },
    } as never)
    expect(await readMultiRenameNames()).toEqual({ ok: false, error: { type: 'noFile' } })
  })
})
