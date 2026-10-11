/**
 * Headless tests for `createDeleteScanState`: when the online-only answer a confirm waits on
 * settles, and what the tallies and preview id do along the way. The dialog-level suites
 * (`DeleteDialog.cloud-storage`, `.early-close`) cover the same paths through the UI; these pin
 * the factory's own contract without mounting anything.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as commands from '$lib/tauri-commands'
import type { ScanPreviewCompleteEvent, ScanPreviewProgressEvent } from '$lib/ipc/bindings'
import { holdOperationStartReply } from '../transfer/transfer-dispatch'
import { createDeleteScanState } from './delete-scan-state.svelte'

vi.mock('$lib/tauri-commands', () => ({
  startScanPreview: vi.fn(),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn(),
  onScanPreviewComplete: vi.fn(),
  onScanPreviewError: vi.fn(),
  onScanPreviewCancelled: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('../transfer/transfer-dispatch', () => ({
  holdOperationStartReply: vi.fn(),
}))

type Handler<E> = (event: E) => void

/** Lets every pending continuation run. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((done) => setTimeout(done, 0))
  }
}

/** Resolves to `true` if `promise` settled within the current macrotask flush. */
async function hasSettled(promise: Promise<unknown>): Promise<boolean> {
  let settled = false
  void promise.then(() => {
    settled = true
  })
  await settle()
  return settled
}

function progress(previewId: string, onlineOnlyFound: boolean): ScanPreviewProgressEvent {
  return {
    previewId,
    filesFound: 3,
    dirsFound: 1,
    bytesFound: 300,
    currentDir: '/cloud/folder',
    onlineOnlyFound,
  } as ScanPreviewProgressEvent
}

function complete(previewId: string, onlineOnlyFound: boolean): ScanPreviewCompleteEvent {
  return { previewId, filesTotal: 5, dirsTotal: 2, bytesTotal: 500, onlineOnlyFound } as ScanPreviewCompleteEvent
}

describe('createDeleteScanState', () => {
  let onProgress: Handler<ScanPreviewProgressEvent>
  let onComplete: Handler<ScanPreviewCompleteEvent>
  let destroyed: boolean

  beforeEach(() => {
    vi.clearAllMocks()
    destroyed = false
    vi.mocked(commands.startScanPreview).mockResolvedValue({ previewId: 'preview-1' })
    vi.mocked(commands.onScanPreviewProgress).mockImplementation((handler) => {
      onProgress = handler
      return Promise.resolve(vi.fn())
    })
    vi.mocked(commands.onScanPreviewComplete).mockImplementation((handler) => {
      onComplete = handler
      return Promise.resolve(vi.fn())
    })
    vi.mocked(commands.onScanPreviewError).mockResolvedValue(vi.fn())
    vi.mocked(commands.onScanPreviewCancelled).mockResolvedValue(vi.fn())
  })

  function create() {
    return createDeleteScanState({
      getSourcePaths: () => ['/cloud/folder'],
      getSortColumn: () => 'name',
      getSortOrder: () => 'ascending',
      getSourceVolumeId: () => 'root',
      getDestroyed: () => destroyed,
    })
  }

  it('answers on the first progress tick that finds online-only content, without waiting for the walk to finish', async () => {
    const scan = create()
    scan.start()
    await scan.scanStarted
    const answer = scan.awaitOnlineOnlyAnswer(undefined)
    expect(await hasSettled(answer)).toBe(false)

    onProgress(progress('preview-1', true))

    expect(await hasSettled(answer)).toBe(true)
    expect(scan.onlineOnlyFoundByWalk).toBe('mixed')
    expect(scan.isScanning).toBe(true)
    expect(scan.filesFound).toBe(3)
    expect(scan.currentDir).toBe('/cloud/folder')
  })

  it('keeps waiting through ticks that find nothing, and answers on the completion', async () => {
    const scan = create()
    scan.start()
    await scan.scanStarted
    const answer = scan.awaitOnlineOnlyAnswer(undefined)

    onProgress(progress('preview-1', false))
    expect(await hasSettled(answer)).toBe(false)

    onComplete(complete('preview-1', false))
    expect(await hasSettled(answer)).toBe(true)
    expect(scan.onlineOnlyFoundByWalk).toBeNull()
    expect(scan.scanComplete).toBe(true)
    expect(scan.bytesFound).toBe(500)
  })

  it('answers when no volume serves the source, so a confirm falls back to the trash', async () => {
    vi.mocked(commands.startScanPreview).mockResolvedValue({ refusal: { kind: 'volumeNotFound' } } as never)
    const scan = create()
    scan.start()
    await scan.scanStarted

    expect(await hasSettled(scan.awaitOnlineOnlyAnswer(undefined))).toBe(true)
    expect(scan.isScanning).toBe(false)
    expect(scan.previewId).toBeNull()
  })

  it('tells a waiting agent to hold for as long as the answer may take', async () => {
    const scan = create()
    scan.start()
    await scan.scanStarted

    void scan.awaitOnlineOnlyAnswer('mcp-7')
    expect(holdOperationStartReply).toHaveBeenCalledWith('mcp-7', 90_000)

    vi.mocked(holdOperationStartReply).mockClear()
    void scan.awaitOnlineOnlyAnswer(undefined)
    expect(holdOperationStartReply).not.toHaveBeenCalled()
  })

  it('ignores events from another preview once its own id is known', async () => {
    const scan = create()
    scan.start()
    await scan.scanStarted

    onProgress(progress('preview-stale', true))

    expect(scan.onlineOnlyFoundByWalk).toBeNull()
    expect(scan.filesFound).toBe(0)
  })

  it('frees only a preview it has an id for', async () => {
    const scan = create()
    scan.freePreview()
    expect(commands.cancelScanPreview).not.toHaveBeenCalled()

    scan.start()
    await scan.scanStarted
    scan.freePreview()
    expect(commands.cancelScanPreview).toHaveBeenCalledWith('preview-1')
  })

  it('releases a waiting confirm on teardown', async () => {
    const scan = create()
    scan.start()
    await scan.scanStarted
    const answer = scan.awaitOnlineOnlyAnswer(undefined)

    scan.settleOnlineOnlyAnswer()

    expect(await hasSettled(answer)).toBe(true)
  })
})
