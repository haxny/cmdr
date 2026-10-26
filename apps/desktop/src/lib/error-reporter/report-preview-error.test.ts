/**
 * The typed reason a report preview file wasn't made survives the throw, and its message carries
 * the detail for the log.
 */

import { describe, expect, it } from 'vitest'
import type { ReportPreviewError } from '#lib/ipc/bindings.js'
import { failureOf } from '#lib/ipc/typed-failure.js'
import { ReportPreviewFailure, throwReportPreviewError } from './report-preview-error'

const every: ReportPreviewError[] = [
  { type: 'bundleUnavailable', detail: 'read log dir: Permission denied (os error 13)' },
  { type: 'notWritten', detail: 'app data dir unknown' },
]

describe('ReportPreviewFailure', () => {
  it.each(every)('carries %j across a throw', (failure) => {
    let caught: unknown
    try {
      throwReportPreviewError(failure)
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(ReportPreviewFailure)
    expect(failureOf(ReportPreviewFailure, caught)).toEqual(failure)
    expect((caught as Error).name).toBe('ReportPreviewFailure')
    expect((caught as Error).message).toBe(`report preview: ${failure.type}: ${failure.detail}`)
  })

  it('reads a failure that isn’t one of its own as null', () => {
    expect(failureOf(ReportPreviewFailure, new Error('bridge broke'))).toBeNull()
  })
})
