/**
 * Tier 3 a11y tests for `ReportOffer.svelte`: the failure button, the inline link, and
 * the slow link.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import ReportOffer from './ReportOffer.svelte'
import type { ReportIncident } from './report-incident'

vi.mock('./error-report-flow.svelte', () => ({ openErrorReportFor: vi.fn() }))
vi.mock('./breadcrumbs', () => ({ recordBreadcrumb: vi.fn() }))

let mounted: ReturnType<typeof mount> | null = null

async function render(props: { incident: ReportIncident; look?: 'button' | 'link' }): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(ReportOffer, { target, props })
  await tick()
  return target
}

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
})

describe('ReportOffer a11y', () => {
  it('the failure button has no violations', async () => {
    await expectNoA11yViolations(await render({ incident: { kind: 'message', from: 'errorToast', text: 'x' } }))
  })

  it('the inline link has no violations', async () => {
    const target = await render({ incident: { kind: 'message', from: 'updateFailure', text: 'x' }, look: 'link' })
    await expectNoA11yViolations(target)
  })

  it('the slow link has no violations', async () => {
    const target = await render({
      incident: { kind: 'slow', operation: 'copy', since: Date.now(), waitingOn: 'destination' },
    })
    await expectNoA11yViolations(target)
  })
})
