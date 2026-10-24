import { describe, expect, it, vi } from 'vitest'
import { createSqliteD1 } from './sqlite-d1-test-helper'
import { countOpsEvent, cronFailureMessage, recordOpsEvent } from './ops-events'

describe('recordOpsEvent', () => {
  it('writes one row the digest will pick up', async () => {
    const { d1, sqlite } = createSqliteD1()

    await recordOpsEvent(d1, 'eviction', 'Evicted 3 bundles')

    const rows = sqlite.prepare(`SELECT kind, message, notified_at FROM ops_events`).all()
    expect(rows).toEqual([{ kind: 'eviction', message: 'Evicted 3 bundles', notified_at: null }])
  })

  it('never throws, because it runs inside the paths whose failures it reports', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const broken = {
      prepare: () => {
        throw new Error('D1 is down')
      },
    } as unknown as D1Database

    await expect(recordOpsEvent(broken, 'cron-failure', 'x')).resolves.toBeUndefined()
    await expect(recordOpsEvent(undefined, 'cron-failure', 'x')).resolves.toBeUndefined()
    vi.restoreAllMocks()
  })
})

describe('countOpsEvent', () => {
  it('folds repeats into the one row the next digest will carry, and starts a fresh one after it', async () => {
    const { d1, sqlite } = createSqliteD1()

    await countOpsEvent(d1, 'admin-auth-rejected', 'GET /admin/stats')
    await countOpsEvent(d1, 'admin-auth-rejected', 'GET /admin/stats')
    await countOpsEvent(d1, 'admin-auth-rejected', 'GET /admin/crashes')
    sqlite.prepare(`UPDATE ops_events SET notified_at = datetime('now')`).run()
    await countOpsEvent(d1, 'admin-auth-rejected', 'GET /admin/stats')

    const rows = sqlite.prepare(`SELECT message, occurrences, notified_at IS NULL AS pending FROM ops_events`).all()
    expect(rows).toEqual([
      { message: 'GET /admin/stats', occurrences: 2, pending: 0 },
      { message: 'GET /admin/crashes', occurrences: 1, pending: 0 },
      { message: 'GET /admin/stats', occurrences: 1, pending: 1 },
    ])
  })

  it('never throws either', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const broken = {
      prepare: () => {
        throw new Error('D1 is down')
      },
    } as unknown as D1Database

    await expect(countOpsEvent(broken, 'admin-auth-rejected', 'x')).resolves.toBeUndefined()
    vi.restoreAllMocks()
  })
})

describe('cronFailureMessage', () => {
  it('names the job, the tick, and the cause, capped so one stack trace cannot bloat the digest', () => {
    const message = cronFailureMessage({
      job: 'Crash cards',
      when: '2026-10-14T03:00:00.000Z',
      detail: 'x'.repeat(5000),
      failedTicks: 1,
      firstFailedAt: '2026-10-14T03:00:00.000Z',
    })
    expect(message).toContain('Crash cards')
    expect(message).toContain('2026-10-14T03:00:00.000Z')
    expect(message).not.toContain('in a row')
    expect(message.length).toBeLessThan(1500)
  })

  it('says how long a streak has run, so the digest shows this is no blip', () => {
    const message = cronFailureMessage({
      job: 'Card ratings',
      when: '2026-10-24T21:00:00.000Z',
      detail: 'Error: GitHub issues: listing comments returned 522',
      failedTicks: 2,
      firstFailedAt: '2026-10-24T18:00:00.000Z',
    })
    expect(message).toBe(
      'Cron job "Card ratings" threw on the 2026-10-24T21:00:00.000Z tick (making 2 ticks in a row since 2026-10-24T18:00:00.000Z), so its work didn\'t happen: Error: GitHub issues: listing comments returned 522. The other jobs on that tick still ran. Full stack: the Workers logs.',
    )
  })
})
