/** What a write did, merged across "Write anyway", and the request that re-sends only the would-drop files. */
import { describe, expect, it } from 'vitest'
import type { TagError } from '#lib/ipc/bindings.js'
import type { TagApplyReport } from '#lib/tauri-commands/index.js'
import { failureKey, fileNameOf, outcomeOf, writeAnywayRequest } from './apply-outcome'

function report(over: Partial<TagApplyReport> = {}): TagApplyReport {
  return { applyId: 'A1', written: [], wouldDrop: [], failed: [], changedOnDisk: [], undoUnavailable: false, ...over }
}

describe('apply-outcome', () => {
  it('Write anyway allows frame loss for the would-drop paths only and skips every other path', () => {
    const request = writeAnywayRequest(
      'music',
      [{ path: '/b/2.mp3', items: ['TSOP'] }],
      ['/b/1.mp3', '/b/2.mp3', '/b/3.mp3', '/b/1.mp3'],
    )
    expect(request).toEqual({ kind: 'music', allowFrameLoss: true, skip: ['/b/1.mp3', '/b/3.mp3'] })
  })

  it('one report is its own outcome', () => {
    const outcome = outcomeOf(
      report({
        written: ['/a', '/b'],
        failed: [{ path: '/c', error: { type: 'notSupported' } }],
        changedOnDisk: ['/d'],
        undoUnavailable: true,
      }),
    )
    expect(outcome).toEqual({
      written: 2,
      failed: [{ path: '/c', error: { type: 'notSupported' } }],
      changedOnDisk: ['/d'],
      skipped: [],
      undoUnavailable: true,
    })
  })

  it('Write anyway adds its own report; skipped would-drop files are listed as skipped', () => {
    const first = report({ written: ['/a'], wouldDrop: [{ path: '/b', items: ['TSOP'] }], changedOnDisk: ['/d'] })
    const second = report({ written: ['/b'], undoUnavailable: true })
    expect(outcomeOf(first, { second })).toEqual({
      written: 2,
      failed: [],
      changedOnDisk: ['/d'],
      skipped: [],
      undoUnavailable: true,
    })
    expect(outcomeOf(first, { skipped: true }).skipped).toEqual([{ path: '/b', items: ['TSOP'] }])
  })

  it.each<[TagError, string]>([
    [{ type: 'notSupported' }, 'multiRename.tags.write.error.notSupported'],
    [{ type: 'io', data: ['PermissionDenied', 'denied'] }, 'multiRename.tags.write.error.io'],
    [{ type: 'parse', data: 'x' }, 'multiRename.tags.write.error.parse'],
    [{ type: 'write', data: 'x' }, 'multiRename.tags.write.error.write'],
    [{ type: 'wouldDrop', data: ['TSOP'] }, 'multiRename.tags.write.error.wouldDrop'],
    [{ type: 'unwritableExtra', data: 'K' }, 'multiRename.tags.write.error.unwritableExtra'],
  ])('a failed file (%j) is worded by its kind', (error, key) => {
    expect(failureKey(error)).toBe(key)
  })

  it('a path shows as its file name', () => {
    expect(fileNameOf('/Books/Kniha/01 Prvni.mp3')).toBe('01 Prvni.mp3')
    expect(fileNameOf('plain.mp3')).toBe('plain.mp3')
  })
})
