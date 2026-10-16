/**
 * A webhook delivery whose signature doesn't verify leaves an ops event, for Paddle and GitHub alike,
 * so the digest can flag a run of them.
 */
import { describe, expect, it } from 'vitest'
import { app } from './index'
import { createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'

function rejections(db: SqliteD1): { message: string; occurrences: number }[] {
  return db.sqlite
    .prepare(`SELECT message, occurrences FROM ops_events WHERE kind = 'webhook-signature-rejected' ORDER BY id`)
    .all() as { message: string; occurrences: number }[]
}

describe('a webhook with a bad signature', () => {
  it('leaves one ops event for Paddle', async () => {
    const db = createSqliteD1()

    const res = await app.request(
      '/webhook/paddle',
      { method: 'POST', headers: { 'Paddle-Signature': 'ts=1704700000;h1=deadbeef' }, body: '{}' },
      { TELEMETRY_DB: db.d1, PADDLE_WEBHOOK_SECRET_LIVE: 'live-secret' },
    )

    expect(res.status).toBe(401)
    const rows = rejections(db)
    expect(rows).toHaveLength(1)
    expect(rows[0].message).toContain('Paddle')
    expect(rows[0].occurrences).toBe(1)
  })

  it('leaves one ops event for GitHub', async () => {
    const db = createSqliteD1()

    const res = await app.request(
      '/webhook/github',
      { method: 'POST', headers: { 'x-hub-signature-256': 'sha256=deadbeef' }, body: '{}' },
      { TELEMETRY_DB: db.d1, GITHUB_WEBHOOK_SECRET: 'gh-secret' },
    )

    expect(res.status).toBe(401)
    const rows = rejections(db)
    expect(rows).toHaveLength(1)
    expect(rows[0].message).toContain('GitHub')
    expect(rows[0].occurrences).toBe(1)
  })
})
