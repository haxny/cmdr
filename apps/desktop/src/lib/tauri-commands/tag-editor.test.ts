/**
 * Tests for the Tags tab's Tauri command wrappers: a typed result passes through as `{ ok, value }` /
 * `{ ok, error }`, and the arguments reach the command unchanged.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    openTagEditor: vi.fn(),
    editTags: vi.fn(),
    runTagTool: vi.fn(),
    getTagChanges: vi.fn(),
    applyTags: vi.fn(),
    undoTagApply: vi.fn(),
    closeTagEditor: vi.fn(),
    listCoverCandidates: vi.fn(),
    setCoverFromFile: vi.fn(),
    getCoverImage: vi.fn(),
  },
}))

import { commands, type ApplyRequest, type TagEditorError } from '#lib/ipc/bindings.js'
import {
  applyTags,
  closeTagEditor,
  editTags,
  getCoverImage,
  getTagChanges,
  listCoverCandidates,
  openTagEditor,
  runTagTool,
  setCoverFromFile,
  undoTagApply,
} from './tag-editor'

const noSession: TagEditorError = { kind: 'noSession' }
const failure = { status: 'error' as const, error: noSession }
const request: ApplyRequest = { kind: 'audiobook', allowFrameLoss: false, skip: [] }

beforeEach(() => {
  vi.clearAllMocks()
})

describe('openTagEditor', () => {
  it('passes the view through and asks for no restart by default', async () => {
    vi.mocked(commands.openTagEditor).mockResolvedValue({ status: 'ok', data: { dirty: false } } as never)
    expect(await openTagEditor('s1')).toEqual({ ok: true, value: { dirty: false } })
    expect(commands.openTagEditor).toHaveBeenCalledWith('s1', false)
  })

  it('passes restart on', async () => {
    vi.mocked(commands.openTagEditor).mockResolvedValue({ status: 'ok', data: {} } as never)
    await openTagEditor('s1', true)
    expect(commands.openTagEditor).toHaveBeenCalledWith('s1', true)
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.openTagEditor).mockResolvedValue(failure)
    expect(await openTagEditor('s1')).toEqual({ ok: false, error: noSession })
  })
})

describe('editTags', () => {
  const edits = [{ kind: 'setAll', field: 'album', values: ['Book'] }] as never

  it('passes the edits and the view through', async () => {
    vi.mocked(commands.editTags).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    expect(await editTags('s1', edits)).toEqual({ ok: true, value: { dirty: true } })
    expect(commands.editTags).toHaveBeenCalledWith('s1', edits)
  })

  it('returns an unknown track as the typed error', async () => {
    vi.mocked(commands.editTags).mockResolvedValue({ status: 'error', error: { kind: 'unknownTrack' } } as never)
    expect(await editTags('s1', edits)).toEqual({ ok: false, error: { kind: 'unknownTrack' } })
  })
})

describe('runTagTool', () => {
  it('passes the tool and the view through', async () => {
    vi.mocked(commands.runTagTool).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    expect(await runTagTool('s1', 'renumber' as never)).toEqual({ ok: true, value: { dirty: true } })
    expect(commands.runTagTool).toHaveBeenCalledWith('s1', 'renumber')
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.runTagTool).mockResolvedValue({ status: 'error', error: { kind: 'readOnly' } } as never)
    expect(await runTagTool('s1', 'renumber' as never)).toEqual({ ok: false, error: { kind: 'readOnly' } })
  })
})

describe('getTagChanges', () => {
  it('passes the changes through', async () => {
    vi.mocked(commands.getTagChanges).mockResolvedValue({ status: 'ok', data: [] } as never)
    expect(await getTagChanges('s1')).toEqual({ ok: true, value: [] })
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.getTagChanges).mockResolvedValue(failure)
    expect(await getTagChanges('s1')).toEqual({ ok: false, error: noSession })
  })
})

describe('applyTags', () => {
  it('passes the request and the whole report through, undo flags included', async () => {
    const report = {
      applyId: 'tags-1-s1',
      written: ['/a.mp3'],
      wouldDrop: [],
      failed: [],
      changedOnDisk: ['/b.mp3'],
      undoUnavailable: false,
    }
    vi.mocked(commands.applyTags).mockResolvedValue({ status: 'ok', data: report } as never)
    expect(await applyTags('s1', request)).toEqual({ ok: true, value: report })
    expect(commands.applyTags).toHaveBeenCalledWith('s1', request)
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.applyTags).mockResolvedValue({ status: 'error', error: { kind: 'noAppDataDir' } } as never)
    expect(await applyTags('s1', request)).toEqual({ ok: false, error: { kind: 'noAppDataDir' } })
  })
})

describe('undoTagApply', () => {
  it('passes the apply id and the restore report through', async () => {
    const report = { restored: ['/a.mp3'], missing: [], changed: [], failed: [] }
    vi.mocked(commands.undoTagApply).mockResolvedValue({ status: 'ok', data: report } as never)
    expect(await undoTagApply('tags-1-s1', 's2')).toEqual({ ok: true, value: report })
    expect(commands.undoTagApply).toHaveBeenCalledWith('tags-1-s1', 's2')
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.undoTagApply).mockResolvedValue(failure)
    expect(await undoTagApply('x', null)).toEqual({ ok: false, error: noSession })
  })
})

describe('closeTagEditor', () => {
  it('closes the session', async () => {
    vi.mocked(commands.closeTagEditor).mockResolvedValue(undefined)
    await closeTagEditor('s1')
    expect(commands.closeTagEditor).toHaveBeenCalledWith('s1')
  })
})

describe('listCoverCandidates', () => {
  it('passes the candidates through', async () => {
    const found = [{ fileName: 'cover.jpg', size: 12 }]
    vi.mocked(commands.listCoverCandidates).mockResolvedValue({ status: 'ok', data: found } as never)
    expect(await listCoverCandidates('s1')).toEqual({ ok: true, value: found })
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.listCoverCandidates).mockResolvedValue(failure)
    expect(await listCoverCandidates('s1')).toEqual({ ok: false, error: noSession })
  })
})

describe('setCoverFromFile', () => {
  it('passes the file name and the view through', async () => {
    vi.mocked(commands.setCoverFromFile).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    expect(await setCoverFromFile('s1', 'cover.jpg')).toEqual({ ok: true, value: { dirty: true } })
    expect(commands.setCoverFromFile).toHaveBeenCalledWith('s1', 'cover.jpg')
  })

  it('returns a refused name as the typed error', async () => {
    vi.mocked(commands.setCoverFromFile).mockResolvedValue({
      status: 'error',
      error: { kind: 'badCoverName' },
    } as never)
    expect(await setCoverFromFile('s1', '../x.jpg')).toEqual({ ok: false, error: { kind: 'badCoverName' } })
  })
})

describe('getCoverImage', () => {
  it('passes the shared cover through', async () => {
    const cover = 'data:image/png;base64,iVBORw0KGgo='
    vi.mocked(commands.getCoverImage).mockResolvedValue({ status: 'ok', data: cover } as never)
    expect(await getCoverImage('s1')).toEqual({ ok: true, value: cover })
    expect(commands.getCoverImage).toHaveBeenCalledWith('s1')
  })

  it('passes "no shared cover" through as null', async () => {
    vi.mocked(commands.getCoverImage).mockResolvedValue({ status: 'ok', data: null } as never)
    expect(await getCoverImage('s1')).toEqual({ ok: true, value: null })
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.getCoverImage).mockResolvedValue(failure)
    expect(await getCoverImage('s1')).toEqual({ ok: false, error: noSession })
  })
})
