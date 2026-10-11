/**
 * Tests for the error-report flow store.
 *
 * Every compose entry point (the Help menu, the toast frame, the shared
 * `ReportOffer` on dialogs, panes, queue rows, and slowness notices) funnels
 * through `openErrorReportDialog`, so this is the right place to assert the
 * open/close contract, the breadcrumbs, and the hand-off to the main window.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import {
  errorReportFlow,
  hostErrorReportDialog,
  openErrorReportDialog,
  openErrorReportDialogForAutoSentReport,
  openErrorReportFor,
  closeErrorReportDialog,
} from './error-report-flow.svelte'

const emitted = vi.hoisted(() => ({ requests: [] as unknown[], handler: null as ((request: unknown) => void) | null }))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setFocus: vi.fn(() => Promise.resolve()) }),
}))

vi.mock('$lib/tauri-commands/error-reporter', () => ({
  requestErrorReportInMainWindow: vi.fn((request: unknown) => {
    emitted.requests.push(request)
    return Promise.resolve()
  }),
  onErrorReportRequested: vi.fn((handler: (request: unknown) => void) => {
    emitted.handler = handler
    return Promise.resolve(() => {
      emitted.handler = null
    })
  }),
}))

function breadcrumbs(): unknown[] {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([command]) => command === 'record_breadcrumb')
    .map(([, args]) => (args as { event: unknown }).event)
}

let unhost: () => void

beforeEach(() => {
  unhost = hostErrorReportDialog()
  closeErrorReportDialog()
  vi.mocked(invoke).mockClear()
  emitted.requests = []
})

afterEach(() => {
  unhost()
})

describe('error-report-flow', () => {
  it('starts closed with an empty initial note', () => {
    expect(errorReportFlow.open).toBe(false)
    expect(errorReportFlow.initialNote).toBe('')
  })

  it('opens with the pre-filled note and records which surface offered it', () => {
    openErrorReportDialog({ from: 'transferError', initialNote: 'something broke' })
    expect(errorReportFlow.open).toBe(true)
    expect(errorReportFlow.initialNote).toBe('something broke')
    expect(breadcrumbs()).toEqual([{ type: 'errorReportDialogOpened', hasInitialNote: true, from: 'transferError' }])
  })

  it('defaults the initial note to empty from the Help menu', () => {
    openErrorReportDialog({ from: 'helpMenu' })
    expect(errorReportFlow.initialNote).toBe('')
    expect(breadcrumbs()).toEqual([{ type: 'errorReportDialogOpened', hasInitialNote: false, from: 'helpMenu' }])
  })

  it('closeErrorReportDialog resets both fields', () => {
    openErrorReportDialog({ from: 'errorToast', initialNote: 'lingering note' })
    closeErrorReportDialog()
    expect(errorReportFlow.open).toBe(false)
    expect(errorReportFlow.initialNote).toBe('')
  })

  it('reopening replaces the initial note', () => {
    openErrorReportDialog({ from: 'errorToast', initialNote: 'first note' })
    closeErrorReportDialog()
    openErrorReportDialog({ from: 'errorToast', initialNote: 'second note' })
    expect(errorReportFlow.initialNote).toBe('second note')
  })

  it('starts in compose mode', () => {
    expect(errorReportFlow.mode).toBe('compose')
    openErrorReportDialog({ from: 'helpMenu' })
    expect(errorReportFlow.mode).toBe('compose')
  })

  it('openErrorReportDialogForAutoSentReport opens in amend mode', () => {
    openErrorReportDialogForAutoSentReport()
    expect(errorReportFlow.open).toBe(true)
    expect(errorReportFlow.mode).toBe('amend')
  })

  it('closeErrorReportDialog drops amend mode', () => {
    openErrorReportDialogForAutoSentReport()
    closeErrorReportDialog()
    expect(errorReportFlow.mode).toBe('compose')
  })

  // The amend entry point exists so the auto-sent toast can't reach the compose
  // path; a stale `amend` left behind would send the Help menu there next time.
  it('the compose entry point resets a leftover amend mode', () => {
    openErrorReportDialogForAutoSentReport()
    openErrorReportDialog({ from: 'helpMenu', initialNote: 'from the Help menu' })
    expect(errorReportFlow.mode).toBe('compose')
  })
})

describe('openErrorReportFor', () => {
  it('opens with the incident’s note and its surface', () => {
    openErrorReportFor({ kind: 'listingFailed', reason: 'networkDown', title: 'The network is down' })
    expect(errorReportFlow.initialNote).toBe('The network is down\nReference: listing/networkDown')
    expect(breadcrumbs()).toEqual([{ type: 'errorReportDialogOpened', hasInitialNote: true, from: 'listingFailure' }])
  })

  // The slowness is the report's whole point, so it lands in the bundle BEFORE the
  // dialog-opened crumb, with the duration the person was looking at.
  it('records the slowness first when the incident is slow', () => {
    const since = Date.now() - 30_000
    openErrorReportFor({ kind: 'slow', operation: 'create_file', since, waitingOn: null })
    expect(breadcrumbs()).toEqual([
      { type: 'operationSlow', operation: 'create_file', stillForSecs: 30, waitingOn: null },
      { type: 'errorReportDialogOpened', hasInitialNote: true, from: 'slowOperation' },
    ])
  })
})

describe('a window without the dialog', () => {
  it('asks the main window to open it instead of opening a dialog nobody renders', () => {
    unhost()
    openErrorReportDialog({ from: 'queueFailure', initialNote: 'Not enough space' })
    expect(errorReportFlow.open).toBe(false)
    expect(emitted.requests).toEqual([{ from: 'queueFailure', initialNote: 'Not enough space' }])
    // The host records the dialog-opened crumb when it opens; the asking window doesn't.
    expect(breadcrumbs()).toEqual([])
  })

  it('still records a slowness crumb in the asking window', () => {
    unhost()
    openErrorReportFor({ kind: 'slow', operation: 'copy', since: Date.now() - 12_000, waitingOn: 'destination' })
    expect(breadcrumbs()).toEqual([
      { type: 'operationSlow', operation: 'copy', stillForSecs: 12, waitingOn: 'destination' },
    ])
    expect(emitted.requests).toHaveLength(1)
  })

  it('the host opens what another window asked for', () => {
    expect(emitted.handler).not.toBeNull()
    emitted.handler?.({ from: 'updateFailure', initialNote: 'The update didn’t download.' })
    expect(errorReportFlow.open).toBe(true)
    expect(errorReportFlow.initialNote).toBe('The update didn’t download.')
    expect(breadcrumbs()).toEqual([{ type: 'errorReportDialogOpened', hasInitialNote: true, from: 'updateFailure' }])
  })
})
