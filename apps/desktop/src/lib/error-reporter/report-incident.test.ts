import { describe, expect, it } from 'vitest'
import { reportEntryFor, reportNoteFor, slowBreadcrumbFor, type ReportIncident } from './report-incident'

const NOW = 1_800_000_000_000

describe('reportEntryFor', () => {
  it('names the surface kind each incident came from', () => {
    const cases: [ReportIncident, string][] = [
      [{ kind: 'message', from: 'errorToast', text: 'x' }, 'errorToast'],
      [{ kind: 'message', from: 'updateFailure', text: 'x' }, 'updateFailure'],
      [
        { kind: 'operationFailed', from: 'transferError', operation: 'copy', error: 'insufficient_space', title: 't' },
        'transferError',
      ],
      [{ kind: 'operationsFailed', count: 4 }, 'queueFailure'],
      [{ kind: 'listingFailed', reason: 'networkDown', title: 't' }, 'listingFailure'],
      [{ kind: 'createRefused', entry: 'folder', refusal: 'nameEmpty', reason: 'r' }, 'createRefusal'],
      [{ kind: 'connectionFailed', reason: 'unreachable', text: 't' }, 'connectionFailure'],
      [{ kind: 'updateNotGenuine', version: '0.53.0', check: 'minisign' }, 'updateFailure'],
      [{ kind: 'slow', operation: 'copy', since: NOW, waitingOn: null }, 'slowOperation'],
      [{ kind: 'pageCrash', detail: 'TypeError: x' }, 'pageCrash'],
    ]
    for (const [incident, from] of cases) expect(reportEntryFor(incident)).toBe(from)
  })
})

describe('reportNoteFor', () => {
  it('names an update that wasn’t genuine by its version and which signature didn’t hold (#457)', () => {
    expect(reportNoteFor({ kind: 'updateNotGenuine', version: '0.53.0', check: 'code_signature' })).toBe(
      'An update offered as version 0.53.0 didn’t carry Cmdr’s signature, so Cmdr didn’t install it.\nReference: update/not_genuine/code_signature',
    )
  })

  it('says a window stopped working, quotes what was thrown, and tags it page/crash', () => {
    expect(reportNoteFor({ kind: 'pageCrash', detail: 'TypeError: undefined is not an object' })).toBe(
      'This Cmdr window stopped working.\nTypeError: undefined is not an object\nReference: page/crash',
    )
  })

  it('leaves the thrown line out of a page crash that carried none', () => {
    expect(reportNoteFor({ kind: 'pageCrash', detail: null })).toBe(
      'This Cmdr window stopped working.\nReference: page/crash',
    )
  })

  it('keeps an already-worded message as the whole note', () => {
    expect(reportNoteFor({ kind: 'message', from: 'errorToast', text: 'Couldn’t open “a.txt”.' })).toBe(
      'Couldn’t open “a.txt”.',
    )
  })

  it('names a stopped operation by its title plus the typed operation and error kind', () => {
    const note = reportNoteFor({
      kind: 'operationFailed',
      from: 'queueFailure',
      operation: 'move',
      error: 'insufficient_space',
      title: 'Not enough space',
    })
    expect(note).toBe('Not enough space\nReference: move/insufficient_space')
  })

  it('references the typed listing reason, and leaves it out when there is none', () => {
    expect(reportNoteFor({ kind: 'listingFailed', reason: 'networkDown', title: 'The network is down' })).toBe(
      'The network is down\nReference: listing/networkDown',
    )
    expect(reportNoteFor({ kind: 'listingFailed', reason: null, title: 'Couldn’t list' })).toBe('Couldn’t list')
  })

  it('references a refused create by entry kind and the typed refusal', () => {
    expect(
      reportNoteFor({ kind: 'createRefused', entry: 'file', refusal: 'parentNotWritable', reason: 'No permission.' }),
    ).toBe('No permission.\nReference: create_file/parentNotWritable')
  })

  it('references a connection failure by its typed reason', () => {
    expect(reportNoteFor({ kind: 'connectionFailed', reason: 'tooSlow', text: 'The server took too long.' })).toBe(
      'The server took too long.\nReference: smb/tooSlow',
    )
  })

  it('counts a collapsed burst of failures', () => {
    expect(reportNoteFor({ kind: 'operationsFailed', count: 4 })).toBe('4 operations couldn’t finish')
  })

  it('says what is slow, for how long as of now, and which side it waits on', () => {
    const note = reportNoteFor({ kind: 'slow', operation: 'copy', since: NOW - 45_000, waitingOn: 'destination' }, NOW)
    expect(note).toBe(
      'Copying: taking long, no progress for 45s.\nWaiting for the destination to respond.\nReference: copy/slow/destination',
    )
  })

  it('words a slow create without a waiting side', () => {
    const note = reportNoteFor({ kind: 'slow', operation: 'create_folder', since: NOW - 6_000, waitingOn: null }, NOW)
    expect(note).toBe('Creating folder: taking long, no progress for 6s.\nReference: create_folder/slow')
  })
})

describe('slowBreadcrumbFor', () => {
  it('carries the typed operation, the whole seconds, and the waiting side', () => {
    expect(
      slowBreadcrumbFor({ kind: 'slow', operation: 'move', since: NOW - 12_400, waitingOn: 'source' }, NOW),
    ).toEqual({
      type: 'operationSlow',
      operation: 'move',
      stillForSecs: 12,
      waitingOn: 'source',
    })
  })

  it('never reports negative time for a clock that stepped back', () => {
    expect(
      slowBreadcrumbFor({ kind: 'slow', operation: 'copy', since: NOW + 5_000, waitingOn: null }, NOW).stillForSecs,
    ).toBe(0)
  })
})
