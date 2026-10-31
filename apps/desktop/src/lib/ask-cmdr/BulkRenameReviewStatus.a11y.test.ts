/**
 * Tier-3 a11y test for `BulkRenameReviewStatus.svelte`: the check's progress, the rows it never
 * finished, and a refusal are axe-clean together, and the two announcing regions keep their roles.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'

vi.mock('./ask-cmdr-trigger.svelte', () => ({
  checkRenameReviewAgain: vi.fn(),
  stopRenameChecks: vi.fn(),
}))

import BulkRenameReviewStatus from './BulkRenameReviewStatus.svelte'
import type { BulkRenameReviewProposal } from './ask-cmdr-trigger.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

function proposal(overrides: Partial<BulkRenameReviewProposal>): BulkRenameReviewProposal {
  return {
    expired: false,
    changed: false,
    checkFailed: false,
    checkProgress: null,
    rows: [],
    ...overrides,
  } as BulkRenameReviewProposal
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('BulkRenameReviewStatus a11y', () => {
  it('has no violations with progress, unchecked rows, and a refusal showing', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(BulkRenameReviewStatus, {
      target,
      props: {
        proposals: [
          proposal({ checkProgress: { checked: 2, total: 10 } }),
          proposal({ checkFailed: true, rows: [{}, {}] as BulkRenameReviewProposal['rows'] }),
        ],
        notice: { kind: 'needsAnotherLook' },
      },
    })
    await tick()
    expect(target.querySelector('[role="status"]')).not.toBeNull()
    expect(target.querySelector('[role="alert"]')).not.toBeNull()
    await expectNoA11yViolations(target)
    await unmount(instance)
  })
})
