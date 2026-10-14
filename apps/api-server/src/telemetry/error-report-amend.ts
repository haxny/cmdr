/**
 * Amending an error report that is already in R2: `POST /error-report/:id/amend`.
 *
 * A reporter's second thought (a note they forgot, an address they now want to give) has to land on
 * the report they already sent. Without this route the app's only move is a second upload, which
 * mints a second `ERR-XXXXX` and splits one incident across two unrelated bundles.
 *
 * Two pieces make it work:
 *
 * - **The KV index** (`report:{ERR-XXXXX}` in `ERROR_REPORT_META`): maps a short id to the bundle's
 *   R2 key, so a report is addressable by the id the user can read. It also carries the SHA-256 of
 *   the report's amend credential, never the credential itself.
 * - **The sidecar** (`{bundle key with .zip → .amend.json}`): the amendments themselves, read-modify-
 *   written so a second amendment appends. Living beside the bundle means nothing has to be kept in
 *   step with it: it is evicted with the bundle and expires on the same 90-day R2 lifecycle.
 */

import { Hono, type Context } from 'hono'
import { enforceIpRateLimit, hasEmailShape, isAbsent, readCappedBody, type Bindings } from '../types'
import { constantTimeEqual } from '../licensing/paddle'
import { amendSidecarKey } from './error-report-eviction'
import type { ErrorReportIssueInput } from '../github-issues'

const errorReportAmend = new Hono<{ Bindings: Bindings }>()

/**
 * Matches the client-side `ERR-XXXXX` short id, same alphabet as the upload route
 * (`apps/desktop/src-tauri/src/short_id.rs`).
 */
const SHORT_ID_PATTERN = /^ERR-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{5}$/

/** KV key prefix for the short-id → bundle index. */
export const REPORT_INDEX_PREFIX = 'report:'

/** KV key for one report's index entry. */
export function reportIndexKey(id: string): string {
  return `${REPORT_INDEX_PREFIX}${id}`
}

/**
 * How long an index entry lives: the same 90 days as the R2 bucket lifecycle, so the index never
 * outlives the bundle it points at and never points at one that's already gone.
 */
export const REPORT_INDEX_TTL_SECONDS = 90 * 24 * 60 * 60

/**
 * When everything about a report uploaded on `uploadDate` (`yyyy-mm-dd`) is promised to be gone:
 * the R2 lifecycle's 90 days, counted from the upload rather than from now.
 *
 * An amendment's card comment carries this rather than its own 90 days. A report amended on day 80
 * would otherwise keep a note on the board until day 170, outliving the bundle by months and the
 * privacy policy's "anything you added to the report afterwards" with it.
 */
export function reportExpiryDate(uploadDate: string): string {
  const uploaded = Date.parse(`${uploadDate}T00:00:00Z`)
  const base = Number.isNaN(uploaded) ? Date.now() : uploaded
  return new Date(base + REPORT_INDEX_TTL_SECONDS * 1000).toISOString().slice(0, 10)
}

/** What the index knows about one uploaded report. */
export interface ReportIndexEntry {
  env: 'prod' | 'dev'
  /** The `yyyy-mm-dd` segment of the key, so a human reading a KV dump sees when it landed. */
  date: string
  /** The full R2 object key of the bundle. */
  key: string
  /** SHA-256 (hex) of the amend credential handed to the client once, at upload. */
  amendKeyHash: string
}

/**
 * A fresh amend credential: 32 random bytes, base64url, returned to the client exactly once.
 *
 * Why a credential at all, rather than treating the id as proof: `ERR-XXXXX` is 31^5 ≈ 28.6 million
 * values, and it's printed in the send dialog, quoted in emails, and pasted into chats. It
 * identifies a report; it can't authorize writing to one.
 */
export function mintAmendKey(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32))
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '')
}

/** SHA-256 of an amend key, hex. The only form of the credential that is ever stored. */
export async function hashAmendKey(amendKey: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(amendKey))
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

/** Write one report's index entry. Throws on a KV failure; the caller decides what that costs. */
export async function writeReportIndex(kv: KVNamespace, id: string, entry: ReportIndexEntry): Promise<void> {
  await kv.put(reportIndexKey(id), JSON.stringify(entry), { expirationTtl: REPORT_INDEX_TTL_SECONDS })
}

/** Read one report's index entry, or null when the id was never indexed or has expired. */
export async function readReportIndex(kv: KVNamespace, id: string): Promise<ReportIndexEntry | null> {
  const raw = await kv.get(reportIndexKey(id))
  if (raw === null) return null
  try {
    return JSON.parse(raw) as ReportIndexEntry
  } catch {
    console.error(`Error report amend: index entry for ${id} is not JSON`)
    return null
  }
}

/** One thing a reporter added after the fact. */
export interface Amendment {
  note: string | null
  email: string | null
  /** ISO 8601 UTC, server clock. The client's is not trusted for an ordering signal. */
  amendedAt: string
}

/** The sidecar object's whole content. */
export interface AmendSidecar {
  id: string
  amendments: Amendment[]
}

/**
 * Byte cap on the amendment body, enforced against bytes actually read. A note is capped
 * client-side at 100,000 code points (the same limit the send dialog and `/feedback` use), which is
 * at most ~400 KB of UTF-8, so 512 KB leaves room for the JSON envelope.
 */
const MAX_AMEND_BODY_BYTES = 512 * 1024

/** Hard cap on the note, counted in code points so it matches the desktop validators. */
const MAX_NOTE_CHARS = 100_000

/** A validated amendment request, once the body has passed every shape check. */
interface AmendRequest {
  amendKey: string
  note: string | null
  email: string | null
}

/**
 * Validate the JSON body. Returns the error message to surface as a 400, or the request.
 *
 * Optional fields tolerate `null` as well as `undefined`: serde serializes `Option::None` as
 * `null`, and a `!== undefined`-only check would reject exactly the note-less amendments a client
 * sends when someone only wants to add their address.
 */
function validateAmendBody(body: Record<string, unknown>): string | AmendRequest {
  const amendKey = body['amendKey']
  if (typeof amendKey !== 'string' || amendKey.length === 0) {
    return 'Missing amend key'
  }

  const note = validateNote(body['note'])
  if (isFieldError(note)) return note.message

  const email = validateReplyTo(body['email'])
  if (isFieldError(email)) return email.message

  // Nothing to add is not an amendment. Accepting it would write an empty entry and mail a blank
  // card, so say what's missing instead.
  if (note === null && email === null) {
    return 'Nothing to add: send a note, an email address, or both'
  }

  return { amendKey, note, email }
}

/** What a field validator returns when the value is present but wrong. */
interface FieldError {
  message: string
}

function isFieldError(value: string | null | FieldError): value is FieldError {
  return value !== null && typeof value === 'object'
}

/** The trimmed note, `null` when absent or blank, or the 400 message. */
function validateNote(raw: unknown): string | null | FieldError {
  if (isAbsent(raw)) return null
  if (typeof raw !== 'string') return { message: 'Invalid note' }
  const note = raw.trim()
  if (note.length === 0) return null
  // Code points, not UTF-16 units, so the cap matches the desktop validators.
  if (Array.from(note).length > MAX_NOTE_CHARS) {
    return { message: `Note is too long (max ${String(MAX_NOTE_CHARS)} characters)` }
  }
  return note
}

/** The reply-to address, `null` when absent, or the 400 message. */
function validateReplyTo(raw: unknown): string | null | FieldError {
  if (isAbsent(raw)) return null
  if (typeof raw !== 'string' || !hasEmailShape(raw)) return { message: 'Invalid email' }
  return raw
}

/**
 * Read and parse the JSON body under the size cap. Returns the parsed object or the error to send.
 *
 * Reads the stream through `readCappedBody` rather than `c.req.text()`, for the reason spelled out
 * there: `content-length` is advisory, so a caller who omits or understates it would otherwise get
 * the whole body buffered inside the isolate before any cap could look at it. The header check
 * below is a cheap fast-fail for an honest client, ❌ never the cap itself.
 */
async function readAmendBody(c: Context<{ Bindings: Bindings }>): Promise<Record<string, unknown> | Response> {
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > MAX_AMEND_BODY_BYTES) {
    return c.json({ error: 'Amendment too large' }, 413)
  }

  const rawBody = c.req.raw.body
  if (!rawBody) {
    return c.json({ error: 'Missing request body' }, 400)
  }

  const bytes = await readCappedBody(rawBody, MAX_AMEND_BODY_BYTES)
  if (!bytes) {
    return c.json({ error: 'Amendment too large' }, 413)
  }

  let parsed: unknown
  try {
    parsed = JSON.parse(new TextDecoder().decode(bytes))
  } catch {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  if (!parsed || typeof parsed !== 'object') {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  return parsed as Record<string, unknown>
}

/**
 * Append one amendment to a bundle's sidecar, creating it on the first one.
 *
 * Read-modify-write, not append: R2 has no append. Two amendments racing on one report could
 * therefore lose the earlier one, which is a fine trade here (a person adding two notes to the same
 * report in the same second isn't a case worth a lock, and the email carries the text regardless).
 */
async function appendAmendment(
  bucket: R2Bucket,
  bundleKey: string,
  id: string,
  amendment: Amendment,
): Promise<AmendSidecar> {
  const sidecarKey = amendSidecarKey(bundleKey)

  let existing: AmendSidecar | null = null
  const stored = await bucket.get(sidecarKey)
  if (stored) {
    try {
      existing = await stored.json()
    } catch {
      // A sidecar we can't parse is a sidecar we'd otherwise have to throw the new note away over.
      // Start a fresh one and keep going; the old bytes are gone either way.
      console.error(`Error report amend: sidecar for ${id} is not JSON, starting a new one`)
    }
  }

  const sidecar: AmendSidecar = {
    id,
    amendments: [...(existing?.amendments ?? []), amendment],
  }
  await bucket.put(sidecarKey, JSON.stringify(sidecar), {
    httpMetadata: { contentType: 'application/json' },
    customMetadata: { id, kind: 'amendment' },
  })
  return sidecar
}

/**
 * The report's technical facts, read back off the bundle, for the card an amendment has to create
 * (`report-outbox.ts`, for a report that predates the outbox). Null when R2 no longer holds the
 * object, which means no card: one that can't say what report it is about helps nobody.
 *
 * The bundle's own `customMetadata` is where these live; the index entry carries only the key, the
 * date, and the credential hash. `buildMode` comes from the key's env segment, which the upload
 * route derives from exactly that field.
 */
export async function readReportFacts(
  bucket: R2Bucket,
  entry: Pick<ReportIndexEntry, 'env' | 'key'>,
  args: { id: string; email: string | null },
): Promise<ErrorReportIssueInput | null> {
  const object = await bucket.head(entry.key)
  if (!object) return null

  const meta: Record<string, string | undefined> = object.customMetadata ?? {}
  return {
    id: args.id,
    // A bundle stored without the field reads as an auto-send, the commoner case by far, and the
    // word only labels the card.
    kind: meta.kind === 'user' ? 'user' : 'auto',
    buildMode: entry.env === 'dev' ? 'debug' : 'release',
    appVersion: meta.appVersion ?? 'unknown',
    osVersion: meta.osVersion ?? 'unknown',
    arch: meta.arch ?? 'unknown',
    sizeBytes: object.size,
    r2Key: entry.key,
    uploadedUnixSeconds: Math.floor(object.uploaded.getTime() / 1000),
    // Drives the card's `needs-reply` label only. The address itself goes in the expiring comment.
    email: args.email,
  }
}

/**
 * Queue one amendment for the cron (`report-outbox.ts` posts it to the board, the digest mails it).
 * Awaited before the 200: the sidecar alone reaches nobody, so a failed write is a 502 the client
 * can retry rather than a note that silently goes nowhere.
 */
async function queueAmendment(
  db: D1Database,
  args: { id: string; entry: ReportIndexEntry; amendment: Amendment; amendmentCount: number },
): Promise<void> {
  await db
    .prepare(
      `INSERT INTO error_report_amendments (short_id, report_date, env, r2_key, amendment_count, note, email)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
    )
    .bind(
      args.id,
      args.entry.date,
      args.entry.env,
      args.entry.key,
      args.amendmentCount,
      args.amendment.note,
      args.amendment.email,
    )
    .run()
}

errorReportAmend.post('/error-report/:id/amend', async (c) => {
  // Rate-limit before anything else, on the amend route's own binding. Looser than the upload
  // limiter (this stores a note, not a bundle) and separate from it, so a reporter who just used
  // their upload allowance can still add a note.
  const limited = await enforceIpRateLimit(c.env.ERROR_REPORT_AMEND_LIMITER, c.req)
  if (limited) return limited

  const id = c.req.param('id')
  if (!SHORT_ID_PATTERN.test(id)) {
    return c.json({ error: 'Invalid report id' }, 400)
  }

  const parsed = await readAmendBody(c)
  if (parsed instanceof Response) return parsed

  const validated = validateAmendBody(parsed)
  if (typeof validated === 'string') {
    return c.json({ error: validated }, 400)
  }

  const entry = await readReportIndex(c.env.ERROR_REPORT_META, id)
  if (!entry) {
    return c.json({ error: "We can't find that report" }, 404)
  }

  const presented = await hashAmendKey(validated.amendKey)
  if (!constantTimeEqual(presented, entry.amendKeyHash)) {
    return c.json({ error: "That key doesn't open this report" }, 401)
  }

  const amendment: Amendment = {
    note: validated.note,
    email: validated.email,
    amendedAt: new Date().toISOString(),
  }

  const sidecar = await appendAmendment(c.env.ERROR_REPORTS_BUCKET, entry.key, id, amendment)

  // The card comment and the email are the cron's job (`report-outbox.ts`, `report-digest.ts`). The
  // daily byte budget is deliberately NOT charged: a note is bytes-tiny.
  try {
    await queueAmendment(c.env.TELEMETRY_DB, { id, entry, amendment, amendmentCount: sidecar.amendments.length })
  } catch (e) {
    console.error('Error report amend: queueing the amendment failed', e)
    return c.json({ error: "We couldn't save that right now. Try again in a minute." }, 502)
  }

  return c.json({ id, amendments: sidecar.amendments.length })
})

export { errorReportAmend }
