/**
 * The retention sweep's promise for the report outbox, against a REAL SQLite: an error report's row
 * (note, reply-to) goes with the bundle's 90 days, and an amendment goes with the report it amends.
 */
import { describe, expect, it } from 'vitest'
import { createSqliteD1 } from './sqlite-d1-test-helper'
import { handleRetentionSweep } from './scheduled'
import type { Bindings } from './types'

function daysAgo(days: number): string {
  return new Date(Date.now() - days * 86_400_000).toISOString().slice(0, 19).replace('T', ' ')
}

describe('handleRetentionSweep on the report outbox', () => {
  it('deletes error reports and amendments past 90 days, keeping younger ones', async () => {
    const { d1, sqlite } = createSqliteD1()
    const insertReport = sqlite.prepare(
      `INSERT INTO error_reports (short_id, created_at, kind, build_mode, app_version, os_version, arch, size_bytes, r2_key, user_note)
         VALUES (?, ?, 'user', 'release', '0.52.0', '27.0', 'aarch64', 1, 'k', 'a note')`,
    )
    insertReport.run('ERR-OLD22', daysAgo(95))
    insertReport.run('ERR-NEW22', daysAgo(10))

    const insertAmendment = sqlite.prepare(
      `INSERT INTO error_report_amendments (short_id, created_at, report_date, env, r2_key, amendment_count, note)
         VALUES (?, ?, ?, 'prod', 'k', 1, 'later note')`,
    )
    // Written recently, but about a report from 95 days ago: it goes with the report.
    insertAmendment.run('ERR-OLD22', daysAgo(5), daysAgo(95).slice(0, 10))
    insertAmendment.run('ERR-NEW22', daysAgo(5), daysAgo(10).slice(0, 10))

    await handleRetentionSweep({ TELEMETRY_DB: d1 } as Bindings)

    const reports = sqlite.prepare(`SELECT short_id FROM error_reports`).all() as { short_id: string }[]
    const amendments = sqlite.prepare(`SELECT short_id FROM error_report_amendments`).all() as { short_id: string }[]
    expect(reports.map((r) => r.short_id)).toEqual(['ERR-NEW22'])
    expect(amendments.map((a) => a.short_id)).toEqual(['ERR-NEW22'])
  })
})
