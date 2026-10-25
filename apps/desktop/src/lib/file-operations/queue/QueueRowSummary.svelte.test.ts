/**
 * The summary's layout contract: it stays inside its grid cell, so the status
 * cell beside it ("Running") is never drawn over (#701).
 *
 * The unit environment carries no CSS (`css` off in `vitest.config.ts`), so the
 * rules are read off the stylesheet, next to a DOM check of the shape they apply to.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import QueueRowSummary from './QueueRowSummary.svelte'
import queueRowSummarySource from './QueueRowSummary.svelte?raw'
import type { OperationItems, OperationSnapshot } from '#lib/ipc/bindings.js'

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Dropbox/report.pdf',
    destination: '/Volumes/Backup/report.pdf',
    rollbackSupport: 'archiveRewrite',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy: null,
    startedAt: null,
    items: null,
    ...over,
  }
}

const items: OperationItems = {
  count: 1,
  kinds: { files: 0, folders: 1 },
  names: ['Photos'],
  from: '~/Dropbox',
  to: 'cmdr-s3-test/target/folder',
}

beforeEach(() => {
  document.body.innerHTML = ''
})

async function mountSummary(snap: OperationSnapshot): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(QueueRowSummary, {
    target,
    props: { id: 'summary-1', label: 'Copying', snapshot: snap, reversalVariant: null },
  })
  await tick()
  return target
}

/** The body of the first rule for `selector` in the component's stylesheet. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const match = new RegExp(`${escaped}\\s*\\{([^}]*)\\}`).exec(queueRowSummarySource)
  if (!match) throw new Error(`no rule for ${selector}`)
  return match[1]
}

describe('QueueRowSummary stays inside its cell', () => {
  it('words the action and its items as one truncatable lead', async () => {
    const target = await mountSummary(snapshot({ items }))
    const lead = target.querySelector('.lead')
    expect(lead?.textContent.replace(/\s+/g, ' ').trim()).toBe('Copying 1 folder')
  })

  it('gives a row without items the same lead', async () => {
    const target = await mountSummary(snapshot())
    expect(target.querySelector('.lead')?.textContent.trim()).toBe('Copying')
  })

  it('lets the paths give way first, then ellipsizes the lead rather than overflowing', () => {
    // The paths shrink to nothing before the lead gives an inch...
    expect(rule('.lead')).toMatch(/flex-shrink:\s*0/)
    // ...and the lead never outgrows the cell: `max-width` caps a non-shrinking
    // item's base size, so past that it ends in "…" instead of running under the
    // status cell.
    expect(rule('.lead')).toMatch(/max-width:\s*100%/)
    expect(rule('.lead')).toMatch(/overflow:\s*hidden/)
    expect(rule('.lead')).toMatch(/text-overflow:\s*ellipsis/)
    expect(rule('.lead')).toMatch(/white-space:\s*nowrap/)
    // The cell clips whatever is left, so nothing can paint over its neighbour.
    expect(rule('.summary-row')).toMatch(/overflow:\s*hidden/)
  })
})
