/**
 * Tests for the error-reporter Tauri command wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    sendErrorReport: vi.fn(),
    amendErrorReport: vi.fn(),
    saveErrorReportToDisk: vi.fn(),
    sendCrashLogReport: vi.fn(),
  },
  events: {
    openErrorReport: { emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) },
    errorReportAutoSent: { listen: vi.fn(() => Promise.resolve(() => {})) },
  },
}))

import { invoke } from '@tauri-apps/api/core'
import { commands, events } from '#lib/ipc/bindings.js'
import {
  amendErrorReport,
  getAutoSentReportPreview,
  onErrorReportAutoSent,
  onErrorReportRequested,
  requestErrorReportInMainWindow,
  prepareErrorReportPreview,
  sendErrorReport,
  saveErrorReportToDisk,
  sendCrashLogReport,
} from './error-reporter'

describe('error-reporter wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('prepareErrorReportPreview', () => {
    it('forwards undefined for note and email when neither is provided', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ id: 'ERR-AB23X' })
      const result = await prepareErrorReportPreview()
      expect(invoke).toHaveBeenCalledWith('prepare_error_report_preview', { userNote: undefined, email: undefined })
      expect(result).toEqual({ id: 'ERR-AB23X' })
    })

    it('forwards the user note and email when provided', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ id: 'ERR-AB23X', sizeBytes: 1234 })
      await prepareErrorReportPreview('Something broke', 'tester@example.com')
      expect(invoke).toHaveBeenCalledWith('prepare_error_report_preview', {
        userNote: 'Something broke',
        email: 'tester@example.com',
      })
    })

    it('propagates rejection from invoke', async () => {
      vi.mocked(invoke).mockRejectedValueOnce('preview failed')
      await expect(prepareErrorReportPreview()).rejects.toBe('preview failed')
    })
  })

  describe('sendErrorReport', () => {
    it('forwards the note, the email, and the id the preview showed', async () => {
      vi.mocked(commands.sendErrorReport).mockResolvedValueOnce({ status: 'ok', data: { id: 'ERR-XYZ99' } })
      const result = await sendErrorReport('a note', 'tester@example.com', 'ERR-XYZ99')
      expect(commands.sendErrorReport).toHaveBeenCalledWith('a note', 'tester@example.com', 'ERR-XYZ99')
      expect(result).toEqual({ id: 'ERR-XYZ99' })
    })

    it('forwards null for all three when nothing is provided', async () => {
      vi.mocked(commands.sendErrorReport).mockResolvedValueOnce({ status: 'ok', data: { id: 'ERR-XYZ99' } })
      await sendErrorReport()
      expect(commands.sendErrorReport).toHaveBeenCalledWith(null, null, null)
    })
  })

  describe('getAutoSentReportPreview', () => {
    it('returns what Flow B auto-sent, `canAmend` included', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ id: 'ERR-AUTO9', canAmend: true })
      const result = await getAutoSentReportPreview()
      expect(invoke).toHaveBeenCalledWith('get_auto_sent_report_preview')
      expect(result).toEqual({ id: 'ERR-AUTO9', canAmend: true })
    })

    it('returns null when nothing was auto-sent this run', async () => {
      vi.mocked(invoke).mockResolvedValueOnce(null)
      await expect(getAutoSentReportPreview()).resolves.toBeNull()
    })
  })

  describe('amendErrorReport', () => {
    it('takes no id: the backend has exactly one stashed report', async () => {
      vi.mocked(commands.amendErrorReport).mockResolvedValueOnce({ status: 'ok', data: { id: 'ERR-AUTO9' } })
      const result = await amendErrorReport('one more thing', 'tester@example.com')
      expect(commands.amendErrorReport).toHaveBeenCalledWith('one more thing', 'tester@example.com')
      expect(result).toEqual({ id: 'ERR-AUTO9' })
    })

    it('forwards null for both when neither is provided', async () => {
      vi.mocked(commands.amendErrorReport).mockResolvedValueOnce({ status: 'ok', data: { id: 'ERR-AUTO9' } })
      await amendErrorReport()
      expect(commands.amendErrorReport).toHaveBeenCalledWith(null, null)
    })
  })

  describe('saveErrorReportToDisk', () => {
    it('returns the saved file path', async () => {
      vi.mocked(commands.saveErrorReportToDisk).mockResolvedValueOnce({
        status: 'ok',
        data: '/some/path/error-report-debug-20260423T100000Z.zip',
      })
      const result = await saveErrorReportToDisk()
      expect(commands.saveErrorReportToDisk).toHaveBeenCalledWith(null, null, null)
      expect(result).toBe('/some/path/error-report-debug-20260423T100000Z.zip')
    })

    it('writes the zip under the same id the send would have used', async () => {
      vi.mocked(commands.saveErrorReportToDisk).mockResolvedValueOnce({ status: 'ok', data: '/tmp/bundle.zip' })
      await saveErrorReportToDisk('a note', undefined, 'ERR-AB23X')
      expect(commands.saveErrorReportToDisk).toHaveBeenCalledWith('a note', null, 'ERR-AB23X')
    })
  })
})

describe('error-reporter events', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('asks the main window to open the report with the request as sent', async () => {
    await requestErrorReportInMainWindow({ from: 'queueFailure', initialNote: 'Not enough space' })
    expect(events.openErrorReport.emit).toHaveBeenCalledWith({ from: 'queueFailure', initialNote: 'Not enough space' })
  })

  it('hands the main window the request’s payload, not the event envelope', async () => {
    const handler = vi.fn()
    await onErrorReportRequested(handler)
    const listener = vi.mocked(events.openErrorReport.listen).mock.calls[0][0]
    listener({ event: 'open-error-report', id: 1, payload: { from: 'updateFailure', initialNote: 'x' } })
    expect(handler).toHaveBeenCalledWith({ from: 'updateFailure', initialNote: 'x' })
  })

  it('hands the auto-sent listener the report id payload', async () => {
    const handler = vi.fn()
    await onErrorReportAutoSent(handler)
    const listener = vi.mocked(events.errorReportAutoSent.listen).mock.calls[0][0]
    listener({ event: 'error-report-auto-sent', id: 1, payload: { id: 'ERR-AUTO9' } })
    expect(handler).toHaveBeenCalledWith({ id: 'ERR-AUTO9' })
  })
})

describe('sendCrashLogReport', () => {
  it('returns the id of a crash-log report that landed, scoped to the crash', async () => {
    vi.mocked(commands.sendCrashLogReport).mockResolvedValueOnce({ status: 'ok', data: { id: 'ERR-CRSH1' } })
    await expect(sendCrashLogReport('C-12AB', '2026-10-11T20:00:00Z')).resolves.toEqual({ id: 'ERR-CRSH1' })
    expect(commands.sendCrashLogReport).toHaveBeenCalledWith('C-12AB', '2026-10-11T20:00:00Z')
  })

  it('throws the typed reason when the send doesn’t land', async () => {
    vi.mocked(commands.sendCrashLogReport).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'bundleUnavailable', detail: 'no logs' },
    })
    await expect(sendCrashLogReport('C-12AB', '2026-10-11T20:00:00Z')).rejects.toBeInstanceOf(Error)
  })
})
