import { afterEach, describe, expect, it, vi } from 'vitest'
import { app } from '../index'
import { parseCspReports, isActionableViolation } from './csp-report'
import { createSqliteD1 } from '../sqlite-d1-test-helper'

function createKv() {
  const store = new Map<string, string>()
  const kv = {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
  return { kv, store }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  const db = createSqliteD1()
  return {
    CSP_ALERTS: createKv().kv,
    TELEMETRY_DB: db.d1,
    LICENSE_CODES: {} as KVNamespace,
    /** Test-only handle on the engine behind `TELEMETRY_DB`, for reading the ops events back. */
    sqlite: db.sqlite,
    ...overrides,
  }
}

/** The CSP alerts the next digest would carry. */
function cspAlerts(bindings: ReturnType<typeof createBindings>): string[] {
  const rows = bindings.sqlite.prepare(`SELECT message FROM ops_events WHERE kind = 'csp-violation'`).all() as {
    message: string
  }[]
  return rows.map((row) => row.message)
}

/** The legacy `report-uri` shape Firefox and Safari send. */
function legacyReport(fields: Record<string, string>) {
  return JSON.stringify({ 'csp-report': fields })
}

/** The Reporting API shape Chromium sends to a `report-to` endpoint. */
function reportingApiReport(body: Record<string, string>) {
  return JSON.stringify([{ type: 'csp-violation', age: 10, url: body.documentURL, body }])
}

function post(body: string, bindings: ReturnType<typeof createBindings>, contentType = 'application/csp-report') {
  return app.request(
    '/csp-report',
    { method: 'POST', headers: { 'content-type': contentType, 'cf-connecting-ip': '203.0.113.7' }, body },
    bindings,
  )
}

const likeButtonViolation = {
  'document-uri': 'https://getcmdr.com/blog/total-commander-for-macos/?utm_source=x',
  'blocked-uri': 'https://api.getcmdr.com/likes/total-commander-for-macos',
  'effective-directive': 'connect-src',
  'source-file': 'https://getcmdr.com/blog/total-commander-for-macos/',
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('parseCspReports', () => {
  it('reads the legacy report-uri shape', () => {
    expect(parseCspReports(JSON.parse(legacyReport(likeButtonViolation)))).toEqual([
      {
        documentUrl: likeButtonViolation['document-uri'],
        blockedUrl: likeButtonViolation['blocked-uri'],
        directive: 'connect-src',
        sourceFile: likeButtonViolation['source-file'],
      },
    ])
  })

  it('reads the Reporting API shape and skips other report types', () => {
    const payload = [
      ...(JSON.parse(
        reportingApiReport({
          documentURL: 'https://getcmdr.com/',
          blockedURL: 'https://api.getcmdr.com/r-codes.json',
          effectiveDirective: 'connect-src',
        }),
      ) as unknown[]),
      { type: 'deprecation', body: { id: 'x' } },
    ]
    expect(parseCspReports(payload)).toEqual([
      {
        documentUrl: 'https://getcmdr.com/',
        blockedUrl: 'https://api.getcmdr.com/r-codes.json',
        directive: 'connect-src',
        sourceFile: undefined,
      },
    ])
  })

  it('falls back to violated-directive when effective-directive is missing', () => {
    const [v] = parseCspReports({
      'csp-report': {
        'document-uri': 'https://getcmdr.com/',
        'blocked-uri': 'https://x.test/',
        'violated-directive': 'img-src',
      },
    })
    expect(v.directive).toBe('img-src')
  })

  it('reads the disposition and script sample in both shapes', () => {
    const [legacy] = parseCspReports({
      'csp-report': {
        'document-uri': 'https://getcmdr.com/',
        'blocked-uri': 'inline',
        'effective-directive': 'script-src-elem',
        disposition: 'report',
        'script-sample': ';window.__cmdrRReady',
      },
    })
    expect(legacy).toMatchObject({ disposition: 'report', sample: ';window.__cmdrRReady' })

    const [reportingApi] = parseCspReports([
      {
        type: 'csp-violation',
        body: {
          documentURL: 'https://getcmdr.com/',
          blockedURL: 'inline',
          effectiveDirective: 'script-src-elem',
          disposition: 'enforce',
          sample: 'console.log(1)',
        },
      },
    ])
    expect(reportingApi).toMatchObject({ disposition: 'enforce', sample: 'console.log(1)' })
  })

  it('returns nothing for junk', () => {
    expect(parseCspReports(null)).toEqual([])
    expect(parseCspReports('nope')).toEqual([])
    expect(parseCspReports({ other: 1 })).toEqual([])
    expect(parseCspReports([{ type: 'csp-violation' }])).toEqual([])
  })
})

describe('isActionableViolation', () => {
  const base = {
    documentUrl: 'https://getcmdr.com/blog/x/',
    blockedUrl: 'https://api.getcmdr.com/likes/x',
    directive: 'connect-src',
  }

  it('flags a blocked request from our own page', () => {
    expect(isActionableViolation(base)).toBe(true)
    expect(isActionableViolation({ ...base, documentUrl: 'https://www.getcmdr.com/' })).toBe(true)
  })

  it('ignores pages that are not ours', () => {
    expect(isActionableViolation({ ...base, documentUrl: 'https://evil.test/' })).toBe(false)
    expect(isActionableViolation({ ...base, documentUrl: 'http://localhost:4321/' })).toBe(false)
  })

  it.each(['inline', 'eval', 'data', 'blob', 'chrome-extension://abc/x.js', 'moz-extension://abc/x.js', ''])(
    'ignores the non-network blocked value %j',
    (blockedUrl) => {
      expect(isActionableViolation({ ...base, blockedUrl })).toBe(false)
    },
  )

  it('ignores violations raised by browser extension code', () => {
    expect(isActionableViolation({ ...base, sourceFile: 'chrome-extension://abc/content.js' })).toBe(false)
    expect(isActionableViolation({ ...base, sourceFile: 'safari-web-extension://abc/content.js' })).toBe(false)
  })

  it('ignores blocked fonts, which only extensions and browsers request', () => {
    const font = { ...base, directive: 'font-src', blockedUrl: 'https://fonts.gstatic.com/s/opensans/v44/x.woff2' }
    expect(isActionableViolation(font)).toBe(false)
    // Regression anchor: the scite extension's font arrived attributed to our own PostHog recorder script.
    expect(
      isActionableViolation({ ...font, sourceFile: 'https://getcmdr.com/ph/static/1.435.3/posthog-recorder.js' }),
    ).toBe(false)
  })

  it("ignores Paddle.js loading Retain's script, which we block on purpose", () => {
    const retain = {
      ...base,
      directive: 'script-src-elem',
      blockedUrl: 'https://public.profitwell.com/js/profitwell.js',
    }
    expect(isActionableViolation(retain)).toBe(false)
    expect(isActionableViolation({ ...retain, directive: 'script-src' })).toBe(false)
    // Only that script: anything else from the origin still alerts.
    expect(isActionableViolation({ ...retain, directive: 'connect-src' })).toBe(true)
    expect(isActionableViolation({ ...retain, blockedUrl: 'https://public.profitwell.com/js/other.js' })).toBe(true)
  })
})

describe('POST /csp-report', () => {
  it('raises one ops alert per directive and blocked origin, and answers 204', async () => {
    const { kv, store } = createKv()
    const bindings = createBindings({ CSP_ALERTS: kv })

    const first = await post(legacyReport(likeButtonViolation), bindings)
    const second = await post(
      legacyReport({ ...likeButtonViolation, 'blocked-uri': 'https://api.getcmdr.com/likes/other-post' }),
      bindings,
    )

    expect(first.status).toBe(204)
    expect(second.status).toBe(204)
    const alerts = cspAlerts(bindings)
    expect(alerts).toHaveLength(1)
    expect(alerts[0]).toContain('connect-src')
    expect(alerts[0]).toContain('https://api.getcmdr.com/likes/total-commander-for-macos')
    expect(alerts[0]).toContain('https://getcmdr.com/blog/total-commander-for-macos/')
    // Query strings can carry anything (tracking params, tokens), so they never reach the alert.
    expect(alerts[0]).not.toContain('utm_source')
    expect([...store.keys()]).toEqual(['csp:connect-src:https://api.getcmdr.com'])
  })

  it('accepts the Reporting API content type', async () => {
    const bindings = createBindings()

    const res = await post(
      reportingApiReport({
        documentURL: 'https://getcmdr.com/',
        blockedURL: 'https://api.getcmdr.com/r-codes.json',
        effectiveDirective: 'connect-src',
      }),
      bindings,
      'application/reports+json',
    )

    expect(res.status).toBe(204)
    expect(cspAlerts(bindings)).toHaveLength(1)
  })

  it('stays quiet for noise and still answers 204', async () => {
    const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })))
    vi.stubGlobal('fetch', fetchMock)
    const { kv, store } = createKv()

    const res = await post(
      legacyReport({ ...likeButtonViolation, 'source-file': 'chrome-extension://abc/content.js' }),
      createBindings({ CSP_ALERTS: kv }),
    )

    expect(res.status).toBe(204)
    expect(fetchMock).not.toHaveBeenCalled()
    expect(store.size).toBe(0)
  })

  it('logs a report-only violation on our page with its sample, and never alerts on it', async () => {
    const log = vi.spyOn(console, 'log').mockImplementation(() => undefined)
    const { kv, store } = createKv()
    const bindings = createBindings({ CSP_ALERTS: kv })

    // Would alert if enforced: a real network URL blocked on our own page.
    const network = await post(legacyReport({ ...likeButtonViolation, disposition: 'report' }), bindings)
    // The strict policy's whole point: an inline script the build didn't hash.
    const inline = await post(
      reportingApiReport({
        documentURL: 'https://getcmdr.com/pricing/?utm_source=x',
        blockedURL: 'inline',
        effectiveDirective: 'script-src-elem',
        disposition: 'report',
        sample: ';Paddle.Initialize({ token: "live_x" })',
        sourceFile: 'https://getcmdr.com/pricing/',
      }),
      bindings,
      'application/reports+json',
    )
    // Someone else's page reporting to us is nobody's concern.
    await post(
      legacyReport({ ...likeButtonViolation, 'document-uri': 'https://evil.test/', disposition: 'report' }),
      bindings,
    )

    expect(network.status).toBe(204)
    expect(inline.status).toBe(204)
    expect(cspAlerts(bindings)).toEqual([])
    expect(store.size).toBe(0)
    const logged = log.mock.calls.map((call) => call[1] as Record<string, unknown>)
    expect(logged).toHaveLength(2)
    expect(logged[1]).toEqual({
      directive: 'script-src-elem',
      blockedUrl: 'inline',
      documentUrl: 'https://getcmdr.com/pricing/',
      sourceFile: 'https://getcmdr.com/pricing/',
      sample: ';Paddle.Initialize({ token: "live_x" })',
    })
    log.mockRestore()
  })

  it('answers 204 to a malformed body without alerting', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    const res = await post('{not json', createBindings())

    expect(res.status).toBe(204)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('refuses an oversized body', async () => {
    const res = await post('x'.repeat(64 * 1024), createBindings())
    expect(res.status).toBe(413)
  })

  it('is rate limited per IP', async () => {
    const bindings = createBindings({ CSP_REPORT_LIMITER: { limit: () => Promise.resolve({ success: false }) } })
    const res = await post(legacyReport(likeButtonViolation), bindings)
    expect(res.status).toBe(429)
  })
})

describe('OPTIONS /csp-report', () => {
  it('lets getcmdr.com send Reporting API reports cross-origin', async () => {
    const res = await app.request(
      '/csp-report',
      { method: 'OPTIONS', headers: { origin: 'https://getcmdr.com', 'access-control-request-method': 'POST' } },
      createBindings(),
    )
    expect(res.status).toBe(204)
    expect(res.headers.get('access-control-allow-origin')).toBe('https://getcmdr.com')
    expect(res.headers.get('access-control-allow-headers')).toBe('Content-Type')
  })

  it('grants nothing to other origins', async () => {
    const res = await app.request(
      '/csp-report',
      { method: 'OPTIONS', headers: { origin: 'https://evil.test' } },
      createBindings(),
    )
    expect(res.headers.get('access-control-allow-origin')).toBeNull()
  })
})
