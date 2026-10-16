/**
 * The one admin gate (`admin-auth.ts`), exercised through the assembled app so every `/admin/` route
 * is covered, including the licensing and link-code ones that live outside `src/admin/`.
 */
import { describe, expect, it, vi } from 'vitest'
import { app } from './index'
import { createSqliteD1, type SqliteD1 } from './sqlite-d1-test-helper'

const token = 'test-admin-token-secret'

function bindings(db: SqliteD1, overrides: Record<string, unknown> = {}) {
  return {
    ADMIN_API_TOKEN: token,
    TELEMETRY_DB: db.d1,
    LICENSE_CODES: { get: vi.fn(() => Promise.resolve(null)), put: vi.fn(), list: vi.fn() } as unknown as KVNamespace,
    LINK_CODES: { get: vi.fn(() => Promise.resolve(null)), put: vi.fn() } as unknown as KVNamespace,
    ...overrides,
  }
}

function rejections(db: SqliteD1): { message: string; occurrences: number }[] {
  return db.sqlite
    .prepare(`SELECT message, occurrences FROM ops_events WHERE kind = 'admin-auth-rejected' ORDER BY id`)
    .all() as { message: string; occurrences: number }[]
}

/** Every concrete `/admin/` route the app serves, with its params filled in. */
function adminRoutes(): { method: string; path: string; pattern: string }[] {
  const seen = new Set<string>()
  const routes: { method: string; path: string; pattern: string }[] = []
  for (const route of app.routes) {
    if (!route.path.startsWith('/admin/') || route.method === 'ALL') continue
    const id = `${route.method} ${route.path}`
    if (seen.has(id)) continue
    seen.add(id)
    routes.push({ method: route.method, pattern: route.path, path: route.path.replace(/:[A-Za-z]+/g, 'x') })
  }
  return routes
}

describe('the admin gate', () => {
  it('finds the admin routes to check', () => {
    expect(adminRoutes().length).toBeGreaterThan(10)
  })

  it.each(adminRoutes())('turns away $method $pattern without a valid token, and records it', async (route) => {
    const db = createSqliteD1()

    const res = await app.request(
      route.path,
      { method: route.method, headers: { Authorization: 'Bearer wrong' } },
      bindings(db),
    )

    expect(res.status).toBe(401)
    const rows = rejections(db)
    expect(rows).toHaveLength(1)
    expect(rows[0].message).toContain(`${route.method} ${route.pattern} `)
    expect(rows[0].occurrences).toBe(1)
  })

  it('records nothing for a request with the right token', async () => {
    const db = createSqliteD1()

    const res = await app.request('/admin/r-codes', { headers: { Authorization: `Bearer ${token}` } }, bindings(db))

    expect(res.status).toBe(200)
    expect(rejections(db)).toEqual([])
  })

  it('folds repeats of the same route into one row with a count, so a burst stays one line', async () => {
    const db = createSqliteD1()
    for (let i = 0; i < 3; i++) await app.request('/admin/stats', {}, bindings(db))
    await app.request('/admin/crashes', {}, bindings(db))

    expect(rejections(db).map((row) => row.occurrences)).toEqual([3, 1])
  })

  it('stores no caller IP and no token, only the route', async () => {
    const db = createSqliteD1()
    await app.request(
      '/admin/stats',
      { headers: { Authorization: 'Bearer guess-123', 'cf-connecting-ip': '203.0.113.7' } },
      bindings(db),
    )

    const [row] = rejections(db)
    expect(row.message).not.toContain('203.0.113.7')
    expect(row.message).not.toContain('guess-123')
  })

  it('rate-limits by caller IP before checking the token', async () => {
    const db = createSqliteD1()
    const limit = vi.fn(() => Promise.resolve({ success: false }))

    const res = await app.request(
      '/admin/stats',
      { headers: { Authorization: `Bearer ${token}`, 'cf-connecting-ip': '203.0.113.7' } },
      bindings(db, { ADMIN_LIMITER: { limit } }),
    )

    expect(res.status).toBe(429)
    expect(limit).toHaveBeenCalledWith({ key: '203.0.113.7' })
  })
})
