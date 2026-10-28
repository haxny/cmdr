/** Tier 3 a11y tests for `TrackSortHeader.svelte`: inactive, ascending, descending, right-aligned. */
import { afterEach, describe, it } from 'vitest'
import type { TrackSort, TrackSortKey } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import TrackSortHeader from './TrackSortHeader.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountHeader(current: TrackSort | null, align: 'left' | 'right' = 'left'): Promise<HTMLElement> {
  const mounted = await mountInto(TrackSortHeader, {
    sortKey: 'length' as TrackSortKey,
    label: 'Length',
    current,
    onClick: () => {},
    align,
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('TrackSortHeader a11y', () => {
  it('inactive has no violations', async () => {
    await expectNoA11yViolations(await mountHeader(null))
  })

  it('active ascending has no violations', async () => {
    await expectNoA11yViolations(await mountHeader({ key: 'length', descending: false }, 'right'))
  })

  it('active descending has no violations', async () => {
    await expectNoA11yViolations(await mountHeader({ key: 'length', descending: true }))
  })
})
