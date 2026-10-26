/**
 * Tests for the send-feedback Tauri command wrapper.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    sendFeedback: vi.fn(),
    writeFeedbackLogsPreviewFile: vi.fn(),
  },
}))

import { commands } from '#lib/ipc/bindings.js'
import { failureOf } from '#lib/ipc/typed-failure.js'
import { ReportPreviewFailure } from '#lib/error-reporter/report-preview-error.js'
import { sendFeedback, writeFeedbackLogsPreviewFile } from './feedback'

describe('sendFeedback wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the text, email, logs choice, and previewed logs id, and returns the typed result', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sent' })
    const result = await sendFeedback('great app', 'tester@example.com', true, 'ERR-AB23X')
    expect(commands.sendFeedback).toHaveBeenCalledWith('great app', 'tester@example.com', true, 'ERR-AB23X')
    expect(result).toEqual({ kind: 'sent' })
  })

  it('coerces an omitted email and logs id to null (Rust Option::None)', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sent' })
    await sendFeedback('great app', undefined, false)
    expect(commands.sendFeedback).toHaveBeenCalledWith('great app', null, false, null)
  })

  it('passes through feedback that went without its logs', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sentWithoutLogs' })
    expect(await sendFeedback('it hung', undefined, true)).toEqual({ kind: 'sentWithoutLogs' })
  })

  it('passes through a soft failure result', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'softFailure' })
    expect(await sendFeedback('great app', undefined, false)).toEqual({ kind: 'softFailure' })
  })

  it('degrades to softFailure when the command throws', async () => {
    vi.mocked(commands.sendFeedback).mockRejectedValueOnce(new Error('IPC down'))
    expect(await sendFeedback('great app', undefined, false)).toEqual({ kind: 'softFailure' })
  })
})

describe('writeFeedbackLogsPreviewFile wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('asks for a fresh report when no id is given, and returns the file', async () => {
    const file = { path: '/tmp/cmdr-feedback-preview.txt', id: 'ERR-AB23X' }
    vi.mocked(commands.writeFeedbackLogsPreviewFile).mockResolvedValueOnce({ status: 'ok', data: file })
    expect(await writeFeedbackLogsPreviewFile()).toEqual(file)
    expect(commands.writeFeedbackLogsPreviewFile).toHaveBeenCalledWith(null)
  })

  it('rewrites the same report when given its id', async () => {
    const file = { path: '/tmp/cmdr-feedback-preview.txt', id: 'ERR-AB23X' }
    vi.mocked(commands.writeFeedbackLogsPreviewFile).mockResolvedValueOnce({ status: 'ok', data: file })
    await writeFeedbackLogsPreviewFile('ERR-AB23X')
    expect(commands.writeFeedbackLogsPreviewFile).toHaveBeenCalledWith('ERR-AB23X')
  })

  it('throws the typed reason when the file wasn’t made', async () => {
    const error = { type: 'notWritten', detail: 'Permission denied' } as const
    vi.mocked(commands.writeFeedbackLogsPreviewFile).mockResolvedValueOnce({ status: 'error', error })
    const caught: unknown = await writeFeedbackLogsPreviewFile().catch((e: unknown) => e)
    expect(failureOf(ReportPreviewFailure, caught)).toEqual(error)
  })
})
