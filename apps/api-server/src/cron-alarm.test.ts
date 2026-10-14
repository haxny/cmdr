/**
 * The cron handler's alarm wiring: a job that throws has to reach a channel a person actually
 * watches. Before this existed, every cron failure went to `console.error` and nowhere else, and a
 * dead `RESEND_API_KEY` would have dropped crash alerts silently for as long as it took someone to
 * notice the quiet. Two channels carry it, failing independently: healthchecks.io (its own email,
 * so it works while D1 or Resend is broken) and an ops event in the next digest.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import worker from './index'
import { createBaseEnv } from './cron-test-helpers'
import { createSqliteD1 } from './sqlite-d1-test-helper'

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- mock stands in for Resend's send; a precise signature adds no test value
const mockSend = vi.fn<any>(() => Promise.resolve({ id: 'test-email-id' }))
vi.mock('resend', () => ({
  Resend: class {
    emails = { send: mockSend }
  },
}))

/** A D1 stand-in that throws the moment a job touches it, so every job on the tick fails. */
function brokenD1(): D1Database {
  return {
    prepare: () => {
      throw new Error('D1_ERROR: no such table: crash_reports')
    },
  } as unknown as D1Database
}

/** Every outbound request, so a test can see the healthchecks ping. */
let posts: { url: string; body: string }[] = []

beforeEach(() => {
  posts = []
  vi.spyOn(console, 'error').mockImplementation(() => undefined)
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string | URL, init?: RequestInit) => {
      posts.push({ url: String(url), body: typeof init?.body === 'string' ? init.body : '' })
      return Promise.resolve(new Response('OK'))
    }),
  )
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

/** With the board configured, every every-tick job touches D1, so each one fails on a broken D1. */
const withBoard = { GITHUB_ISSUES_TOKEN: 'ghp_x', GITHUB_ISSUES_REPO: 'vdavid/cmdr-reports' }

/** The jobs every tick runs, in order, as `index.ts` names them. */
const everyTickJobs = [
  'Crash cards',
  'Error report cards',
  'Feedback cards',
  'Amendments on the board',
  'Report digest',
]

/** A non-midnight tick, so only the every-invocation jobs run. */
const threeAm = { scheduledTime: Date.parse('2026-09-02T03:00:00.000Z') } as ScheduledEvent

function healthPings() {
  return posts.filter((p) => p.url.startsWith('https://hc-ping.com/'))
}

describe('scheduled alarm wiring', () => {
  it('fails the healthcheck and names every job that threw, so later jobs ran too', async () => {
    const env = createBaseEnv({
      TELEMETRY_DB: brokenD1(),
      ...withBoard,
      HEALTHCHECKS_PING_URL: 'https://hc-ping.com/abc',
    })

    await worker.scheduled(threeAm, env as never)

    const pings = healthPings()
    expect(pings).toHaveLength(1)
    expect(pings[0]?.url).toBe('https://hc-ping.com/abc/fail')
    for (const job of everyTickJobs) expect(pings[0]?.body).toContain(job)
  })

  it('puts the failure, with its job, tick, and cause, in an ops event for the next digest', async () => {
    const { d1, sqlite } = createSqliteD1()
    // One pending crash and a GitHub that refuses every write: the crash job throws, D1 stays up.
    sqlite.exec(
      `INSERT INTO crash_reports (hashed_ip, app_version, os_version, arch, signal, top_function, backtrace, build_mode)
         VALUES ('', '0.52.0', '27.0', 'aarch64', 'SIGSEGV', 'cmdr::x', 'bt', 'release')`,
    )
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve(new Response('{}', { status: 500 }))),
    )
    const env = createBaseEnv({ TELEMETRY_DB: d1, ...withBoard, ERROR_REPORT_META: memoryKv() })

    await worker.scheduled(threeAm, env as never)

    const events = sqlite.prepare(`SELECT kind, message FROM ops_events`).all() as { kind: string; message: string }[]
    expect(events).toHaveLength(1)
    expect(events[0].kind).toBe('cron-failure')
    expect(events[0].message).toContain('Crash cards')
    expect(events[0].message).toContain('2026-09-02T03:00:00.000Z')
    expect(events[0].message).toContain('failed to file')
  })

  it('pings the healthcheck clean when every job finished', async () => {
    const env = createBaseEnv({
      // No recipient and no board configured, so every job returns early instead of touching D1.
      CRASH_NOTIFICATION_EMAIL: undefined,
      HEALTHCHECKS_PING_URL: 'https://hc-ping.com/abc',
    })

    await worker.scheduled(threeAm, env as never)

    expect(healthPings()).toEqual([{ url: 'https://hc-ping.com/abc', body: 'All cron jobs finished.' }])
  })

  it('runs the cron unchanged when no alarm is configured', async () => {
    const env = createBaseEnv({ TELEMETRY_DB: brokenD1() })

    await expect(worker.scheduled(threeAm, env as never)).resolves.toBeUndefined()

    expect(posts).toEqual([])
  })

  it('never lets a broken alarm channel take the cron down', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.reject(new Error('ping host is down'))),
    )
    const env = createBaseEnv({
      TELEMETRY_DB: brokenD1(),
      ...withBoard,
      HEALTHCHECKS_PING_URL: 'https://hc-ping.com/abc',
    })

    await expect(worker.scheduled(threeAm, env as never)).resolves.toBeUndefined()
  })
})

/** A KV good enough for the outbox's daily cap. */
function memoryKv(): KVNamespace {
  const store = new Map<string, string>()
  return {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
}
