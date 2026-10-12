/**
 * `ReportOffer` is the one "report this" action every failure and slowness surface renders.
 * A failure gets the "Send error report…" button; something slow gets the lighter
 * "Taking long? Send a report" link and puts its slowness in the bundle as soon as it shows.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { tString } from '#lib/intl/messages.svelte.js'
import ReportOffer from './ReportOffer.svelte'
import type { ReportIncident } from './report-incident'

const { openErrorReportFor, recordBreadcrumb } = vi.hoisted(() => ({
  openErrorReportFor: vi.fn(),
  recordBreadcrumb: vi.fn(),
}))

vi.mock('./error-report-flow.svelte', () => ({ openErrorReportFor }))
vi.mock('./breadcrumbs', () => ({ recordBreadcrumb }))

let mounted: ReturnType<typeof mount> | null = null

function render(props: { incident: ReportIncident; onOpen?: () => void }): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(ReportOffer, { target, props })
  return target
}

beforeEach(() => {
  openErrorReportFor.mockClear()
  recordBreadcrumb.mockClear()
})

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
})

describe('ReportOffer', () => {
  it('offers a failure as “Send error report…” and opens the report for that incident', async () => {
    const incident: ReportIncident = {
      kind: 'operationFailed',
      from: 'transferError',
      operation: 'copy',
      error: 'insufficient_space',
      title: 'Not enough space',
    }
    const onOpen = vi.fn()
    const target = render({ incident, onOpen })
    await tick()

    const button = target.querySelector('button')
    expect(button?.textContent.trim()).toBe(tString('errorReporter.offer.send'))
    button?.click()
    expect(openErrorReportFor).toHaveBeenCalledWith(incident)
    expect(onOpen).toHaveBeenCalledOnce()
    // A failure records nothing on its own: the dialog-opened crumb comes from the flow.
    expect(recordBreadcrumb).not.toHaveBeenCalled()
  })

  it('offers something slow as the lighter link, and records the slowness when it shows', async () => {
    const incident: ReportIncident = {
      kind: 'slow',
      operation: 'create_folder',
      since: Date.now() - 3_000,
      waitingOn: null,
    }
    const target = render({ incident })
    await tick()

    expect(recordBreadcrumb).toHaveBeenCalledWith({
      type: 'operationSlow',
      operation: 'create_folder',
      stillForSecs: 3,
      waitingOn: null,
    })
    const link = target.querySelector<HTMLButtonElement>('button.link-button')
    expect(link?.textContent.trim()).toBe(tString('errorReporter.offer.slow'))
    link?.click()
    expect(openErrorReportFor).toHaveBeenCalledWith(incident)
  })
})
