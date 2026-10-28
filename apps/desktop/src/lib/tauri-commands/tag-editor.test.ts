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
    runTagTextTool: vi.fn(),
    getTagChanges: vi.fn(),
    applyTags: vi.fn(),
    undoTagApply: vi.fn(),
    lastTagApply: vi.fn(),
    closeTagEditor: vi.fn(),
    listCoverCandidates: vi.fn(),
    setCoverFromFile: vi.fn(),
    getCoverImage: vi.fn(),
    listTagCovers: vi.fn(),
    getCoverThumbnail: vi.fn(),
    useCoverForAll: vi.fn(),
    setCoverFromBytes: vi.fn(),
    setCoverFromPath: vi.fn(),
    previewTagsFromNames: vi.fn(),
    tagTrackAudioUrl: vi.fn(),
    releaseTagTrackAudio: vi.fn(),
    openTagsInPlayer: vi.fn(),
  },
}))

import { commands, type ApplyRequest, type TagEditorError } from '#lib/ipc/bindings.js'
import {
  applyTags,
  closeTagEditor,
  editTags,
  getCoverImage,
  getCoverThumbnail,
  getTagChanges,
  listCoverCandidates,
  listTagCovers,
  openTagEditor,
  openTagsInPlayer,
  previewTagsFromNames,
  releaseTagTrackAudio,
  runTagTextTool,
  runTagTool,
  setCoverFromBytes,
  setCoverFromFile,
  setCoverFromPath,
  tagTrackAudioUrl,
  undoTagApply,
  lastTagApply,
  useCoverForAll,
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
    expect(commands.runTagTool).toHaveBeenCalledWith('s1', 'renumber', null)
  })

  it('passes the chosen tracks through', async () => {
    vi.mocked(commands.runTagTool).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    await runTagTool('s1', 'renumber' as never, ['/a.mp3'])
    expect(commands.runTagTool).toHaveBeenCalledWith('s1', 'renumber', ['/a.mp3'])
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.runTagTool).mockResolvedValue({ status: 'error', error: { kind: 'readOnly' } } as never)
    expect(await runTagTool('s1', 'renumber' as never)).toEqual({ ok: false, error: { kind: 'readOnly' } })
  })
})

describe('runTagTextTool', () => {
  const tool = { kind: 'format', field: 'title', mask: '[N3-]' } as const

  it('passes the tool, no scope by default, and the view through', async () => {
    vi.mocked(commands.runTagTextTool).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    expect(await runTagTextTool('s1', tool)).toEqual({ ok: true, value: { dirty: true } })
    expect(commands.runTagTextTool).toHaveBeenCalledWith('s1', tool, null)
  })

  it('passes the chosen tracks through', async () => {
    vi.mocked(commands.runTagTextTool).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    await runTagTextTool('s1', tool, ['/a.mp3'])
    expect(commands.runTagTextTool).toHaveBeenCalledWith('s1', tool, ['/a.mp3'])
  })

  it('returns a bad mask and a bad regex as typed errors', async () => {
    const badMask = { kind: 'badMask', error: { type: 'unclosed', at: 0 } }
    vi.mocked(commands.runTagTextTool).mockResolvedValue({ status: 'error', error: badMask } as never)
    expect(await runTagTextTool('s1', tool)).toEqual({ ok: false, error: badMask })
    vi.mocked(commands.runTagTextTool).mockResolvedValue({ status: 'error', error: { kind: 'badRegex' } } as never)
    expect(await runTagTextTool('s1', tool)).toEqual({ ok: false, error: { kind: 'badRegex' } })
  })
})

describe('previewTagsFromNames', () => {
  const pattern = { kind: 'format', pattern: '%title%' } as const

  it('passes the pattern, the scope, and the rows through', async () => {
    const rows = [{ path: '/a.mp3', fileName: 'a.mp3', fields: null }]
    vi.mocked(commands.previewTagsFromNames).mockResolvedValue({ status: 'ok', data: rows })
    expect(await previewTagsFromNames('s1', pattern)).toEqual({ ok: true, value: rows })
    expect(commands.previewTagsFromNames).toHaveBeenCalledWith('s1', pattern, null)
  })

  it('returns the typed error', async () => {
    const error = { kind: 'badPattern', error: { kind: 'noFields' } } as const
    vi.mocked(commands.previewTagsFromNames).mockResolvedValue({ status: 'error', error })
    expect(await previewTagsFromNames('s1', pattern, ['/a.mp3'])).toEqual({ ok: false, error })
  })
})

describe('getTagChanges', () => {
  it('passes the changes through', async () => {
    vi.mocked(commands.getTagChanges).mockResolvedValue({ status: 'ok', data: [] } as never)
    expect(await getTagChanges('s1', 'music')).toEqual({ ok: true, value: [] })
    expect(commands.getTagChanges).toHaveBeenCalledWith('s1', 'music')
  })

  it('returns the typed error', async () => {
    vi.mocked(commands.getTagChanges).mockResolvedValue(failure)
    expect(await getTagChanges('s1', 'audiobook')).toEqual({ ok: false, error: noSession })
  })
})

describe('lastTagApply', () => {
  it('asks for the folder’s last write, which outlives any window', async () => {
    const last = { applies: [{ applyId: 'a1', written: 3 }], partial: false }
    vi.mocked(commands.lastTagApply).mockResolvedValue({ status: 'ok', data: last } as never)
    expect(await lastTagApply('s1')).toEqual({ ok: true, value: last })
    expect(commands.lastTagApply).toHaveBeenCalledWith('s1')
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
    expect(commands.applyTags).toHaveBeenCalledWith('s1', request, false)
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
    expect(commands.getCoverImage).toHaveBeenCalledWith('s1', null)
  })

  it('asks for the cover a choice names', async () => {
    vi.mocked(commands.getCoverImage).mockResolvedValue({ status: 'ok', data: null } as never)
    await getCoverImage('s1', '00ff')
    expect(commands.getCoverImage).toHaveBeenCalledWith('s1', '00ff')
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

describe('getCoverThumbnail', () => {
  it('asks for the small picture of a cover', async () => {
    vi.mocked(commands.getCoverThumbnail).mockResolvedValue({ status: 'ok', data: 'data:image/png;base64,x' } as never)
    expect(await getCoverThumbnail('s1', '00ff')).toEqual({ ok: true, value: 'data:image/png;base64,x' })
    expect(commands.getCoverThumbnail).toHaveBeenCalledWith('s1', '00ff')
  })
})

describe('the cover choices', () => {
  it('lists every cover with its count', async () => {
    const choices = [{ key: 'none', mime: '', bytes: 0, width: null, height: null, files: 2 }]
    vi.mocked(commands.listTagCovers).mockResolvedValue({ status: 'ok', data: choices } as never)
    expect(await listTagCovers('s1')).toEqual({ ok: true, value: choices })
    expect(commands.listTagCovers).toHaveBeenCalledWith('s1')
  })

  it('uses one for every file, and an unknown key comes back typed', async () => {
    vi.mocked(commands.useCoverForAll).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    expect(await useCoverForAll('s1', '00ff')).toEqual({ ok: true, value: { dirty: true } })
    expect(commands.useCoverForAll).toHaveBeenCalledWith('s1', '00ff')
    vi.mocked(commands.useCoverForAll).mockResolvedValue({ status: 'error', error: { kind: 'unknownCover' } } as never)
    expect(await useCoverForAll('s1', 'gone')).toEqual({ ok: false, error: { kind: 'unknownCover' } })
  })

  it('sends pasted bytes as a plain number array', async () => {
    vi.mocked(commands.setCoverFromBytes).mockResolvedValue({ status: 'ok', data: { dirty: true } } as never)
    expect(await setCoverFromBytes('s1', new Uint8Array([0xff, 0xd8, 0xff]))).toEqual({
      ok: true,
      value: { dirty: true },
    })
    // Base64, not a JSON number array: a large TIFF would cross IPC several times its size.
    expect(commands.setCoverFromBytes).toHaveBeenCalledWith('s1', '/9j/')
  })

  it('sends a dropped path, and a folder comes back typed', async () => {
    vi.mocked(commands.setCoverFromPath).mockResolvedValue({
      status: 'error',
      error: { kind: 'coverNotAFile' },
    } as never)
    expect(await setCoverFromPath('s1', '/x/folder')).toEqual({ ok: false, error: { kind: 'coverNotAFile' } })
    expect(commands.setCoverFromPath).toHaveBeenCalledWith('s1', '/x/folder')
  })
})

describe('tagTrackAudioUrl', () => {
  it('passes the session, the path, and the URL through', async () => {
    const url = 'cmdr-media://localhost/0123456789abcdef0123456789abcdef'
    vi.mocked(commands.tagTrackAudioUrl).mockResolvedValue({ status: 'ok', data: url })
    expect(await tagTrackAudioUrl('s1', '/books/a.mp3')).toEqual({ ok: true, value: url })
    expect(commands.tagTrackAudioUrl).toHaveBeenCalledWith('s1', '/books/a.mp3')
  })

  it('returns a path outside the session as the typed error', async () => {
    const unknown: TagEditorError = { kind: 'unknownTrack' }
    vi.mocked(commands.tagTrackAudioUrl).mockResolvedValue({ status: 'error', error: unknown })
    expect(await tagTrackAudioUrl('s1', '/etc/hosts')).toEqual({ ok: false, error: unknown })
  })
})

describe('releaseTagTrackAudio', () => {
  it('passes the URL through', async () => {
    vi.mocked(commands.releaseTagTrackAudio).mockResolvedValue(undefined)
    await releaseTagTrackAudio('cmdr-media://localhost/abc')
    expect(commands.releaseTagTrackAudio).toHaveBeenCalledWith('cmdr-media://localhost/abc')
  })
})

describe('openTagsInPlayer', () => {
  it('passes the session and the paths through', async () => {
    vi.mocked(commands.openTagsInPlayer).mockResolvedValue({ status: 'ok', data: null })
    expect(await openTagsInPlayer('s1', ['/books/a.mp3', '/books/b.mp3'])).toEqual({ ok: true, value: null })
    expect(commands.openTagsInPlayer).toHaveBeenCalledWith('s1', ['/books/a.mp3', '/books/b.mp3'])
  })

  it('returns a launch that could not start as the typed error', async () => {
    const unavailable: TagEditorError = { kind: 'playerUnavailable' }
    vi.mocked(commands.openTagsInPlayer).mockResolvedValue({ status: 'error', error: unavailable })
    expect(await openTagsInPlayer('s1', ['/books/a.mp3'])).toEqual({ ok: false, error: unavailable })
  })
})
