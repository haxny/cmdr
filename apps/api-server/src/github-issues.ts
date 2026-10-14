/**
 * Files crashes, error reports, and feedback as issues in a PRIVATE GitHub repo, so triage happens on
 * one board. The cron decides WHEN (`report-outbox.ts`); this module decides WHAT goes on a card.
 *
 * Two invariants hold this together, and both are load-bearing for the privacy policy
 * (`apps/website/src/pages/privacy-policy.astro`):
 *
 * 1. **The repo must be private, checked on every write.** {@link fileIssue} probes the repo and
 *    refuses to post anything unless GitHub answers `private: true`. It fails CLOSED: a lookup that
 *    errors, 404s, or omits the field writes nothing. That's what stops a misconfigured
 *    `GITHUB_ISSUES_REPO` (or a repo someone flips to public) from publishing a user's note.
 * 2. **Personal data lives only in a comment that expires.** The issue BODY carries technical facts
 *    that name nobody and may be kept indefinitely. Anything a person wrote or attached goes in a
 *    separate comment stamped with its own deletion date, which the retention sweep deletes on time
 *    (`scheduled.ts`). Deleting a comment removes it outright; editing a body would leave the
 *    original text in GitHub's revision history, which is why the split exists at all.
 *
 * Untrusted text is always wrapped with {@link fencedBlock}, so a note can't become markup, an
 * `@mention`, or a cross-repo reference.
 */

import { formatBytes, type Bindings } from './types'

const GITHUB_API = 'https://api.github.com'

/**
 * GitHub rejects API requests without a User-Agent. Naming the Worker makes a misbehaving
 * integration identifiable in GitHub's own logs rather than anonymous traffic.
 */
const USER_AGENT = 'cmdr-api-server'

/**
 * Hard ceiling GitHub puts on an issue body and on a comment. We stay under it with margin rather
 * than at it, because the chrome around a note is counted too.
 */
const GITHUB_TEXT_LIMIT = 65_536
const SAFE_TEXT_BUDGET = GITHUB_TEXT_LIMIT - 5_536

/**
 * The prefix every personal-data comment starts with. The retention sweep matches on this to find
 * what it may delete, so ❌ never post personal data in a comment without it: an unmarked comment is
 * invisible to the sweep and would outlive its promise.
 */
export const PERSONAL_COMMENT_MARKER = '<!-- cmdr:personal-data'

/**
 * Default life of a personal-data comment, in days. Matches the error-report retention promise, and
 * it is the DEFAULT so that a caller who forgets to state one deletes too early rather than too
 * late.
 */
const DEFAULT_PERSONAL_RETENTION_DAYS = 90

/** Where issues are filed. Absent config means the integration is off, never a fallback repo. */
export interface IssueTarget {
  token: string
  owner: string
  repo: string
}

/**
 * Read the target from config. Returns null (integration off) unless BOTH a token and an
 * `owner/name` repo are present, so a half-configured environment posts nothing instead of guessing.
 */
export function resolveIssueTarget(env: Bindings): IssueTarget | null {
  const token = env.GITHUB_ISSUES_TOKEN
  const repo = env.GITHUB_ISSUES_REPO
  if (!token || !repo) return null

  const parts = repo.split('/')
  if (parts.length !== 2) return null
  const [owner, name] = parts
  if (!owner || !name) return null

  return { token, owner, repo: name }
}

/** Narrow a parsed JSON value to something whose keys can be read. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function githubHeaders(token: string): Record<string, string> {
  return {
    authorization: `Bearer ${token}`,
    accept: 'application/vnd.github+json',
    'x-github-api-version': '2022-11-28',
    'user-agent': USER_AGENT,
  }
}

/**
 * Wrap untrusted text in a code fence long enough that the text cannot close it. Everything inside
 * is inert: no headings, no `@mentions` that would notify a stranger, no `#123` cross-references.
 */
export function fencedBlock(text: string): string {
  let longestRun = 0
  let run = 0
  for (const char of text) {
    run = char === '`' ? run + 1 : 0
    if (run > longestRun) longestRun = run
  }
  const fence = '`'.repeat(Math.max(3, longestRun + 1))
  return `${fence}\n${text}\n${fence}`
}

/** Cut `text` to `maxChars`, marking the cut so nobody reads a truncated note as the whole note. */
function truncate(text: string, maxChars: number): string {
  if (text.length <= maxChars) return text
  return `${text.slice(0, maxChars)}\n\n[truncated at ${String(maxChars)} characters]`
}

/** `YYYY-MM-DD`, `days` from now. The stamp the retention sweep reads. */
function expiryDate(days: number): string {
  return new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10)
}

/**
 * `YYYY-MM-DD`, `days` after the day a row was written. Cards are filed by the cron, possibly days
 * after intake when GitHub was down, so a promise counted from "now" would outlive the one the
 * privacy policy makes, which counts from when the person sent it.
 *
 * Takes either `created_at` shape the tables use (`datetime('now')` or ISO 8601): only the day is read.
 */
export function expiryAfter(createdAt: string, days: number): string {
  const day = Date.parse(`${createdAt.slice(0, 10)}T00:00:00Z`)
  const base = Number.isNaN(day) ? Date.now() : day
  return new Date(base + days * 86_400_000).toISOString().slice(0, 10)
}

/** Read the deletion date off a personal-data comment. Null when it carries no valid stamp. */
export function personalCommentExpiry(commentBody: string): string | null {
  const match = /^<!-- cmdr:personal-data expires=(\d{4}-\d{2}-\d{2}) -->/.exec(commentBody)
  return match?.[1] ?? null
}

/** What a personal-data comment may carry. Every field is optional; all-absent means no comment. */
export interface PersonalCommentInput {
  userNote?: string | null
  email?: string | null
  downloadUrl?: string | null
  linkTtlHours?: number
  retentionDays?: number
  /**
   * An absolute `YYYY-MM-DD` deletion date, overriding `retentionDays`. An amendment uses this to
   * expire with the report it amends rather than 90 days from when it was written, which would
   * outlive the bundle it belongs to.
   */
  expiresOn?: string
}

/**
 * Build the one comment that holds everything a person wrote or attached, stamped with the date the
 * retention sweep must delete it. Returns null when there is nothing personal to store, so an
 * anonymous auto-report never gets an empty comment that would later need deleting.
 */
export function buildPersonalComment(input: PersonalCommentInput): string | null {
  const note = input.userNote?.trim() ?? ''
  const email = input.email?.trim() ?? ''
  const hasLink = Boolean(input.downloadUrl)
  if (!note && !email && !hasLink) return null

  const retentionDays = input.retentionDays ?? DEFAULT_PERSONAL_RETENTION_DAYS
  const expiresOn = input.expiresOn ?? expiryDate(retentionDays)
  const sections: string[] = [
    `${PERSONAL_COMMENT_MARKER} expires=${expiresOn} -->`,
    `_Deleted automatically on ${expiresOn}, per the privacy policy. The issue above stays._`,
  ]

  if (note) {
    sections.push('**What they wrote**', fencedBlock(truncate(note, SAFE_TEXT_BUDGET / 2)))
  }
  if (email) {
    sections.push('**Reply to**', fencedBlock(email))
  }
  if (input.downloadUrl) {
    const ttl = input.linkTtlHours
    const suffix = ttl ? ` (link expires in ${String(ttl)} hours)` : ''
    sections.push('**Bundle**', `[Download the bundle](${input.downloadUrl})${suffix}`)
  }

  return truncate(sections.join('\n\n'), SAFE_TEXT_BUDGET)
}

/**
 * `macOS <version>`, without doubling the prefix when the client already sent one.
 *
 * Older clients send a bare `15.3.1`, some send `macOS 10.15.8`. ERR-KVERS was titled
 * "macOS macOS 10.15.8" before this existed.
 */
function macOsLabel(osVersion: string): string {
  const trimmed = osVersion.trim()
  return /^macos\b/i.test(trimmed) ? trimmed : `macOS ${trimmed}`
}

/** The three parts of an issue, built before anything touches the network. */
export interface IssueContent {
  title: string
  body: string
  labels: string[]
}

/**
 * Every new card starts here. The morning triage agent researches each one, renames it, and takes
 * the label off, so an open card that still carries it is one nobody has looked at yet.
 */
export const UNPROCESSED_LABEL = 'unprocessed'

/** A person attached an address and is waiting on an answer. */
export const NEEDS_REPLY_LABEL = 'needs-reply'

/** The labels every card gets: where it came from, `unprocessed`, and `needs-reply` when someone waits. */
function cardLabels(source: IssueSource, email: string | null | undefined): string[] {
  const labels: string[] = [source === 'feedback' ? 'in-app-feedback' : source, UNPROCESSED_LABEL]
  if (email?.trim()) labels.push(NEEDS_REPLY_LABEL)
  return labels
}

/** Cut a title fragment to `maxChars`, so one long symbol can't push the facts off the end. */
function clip(text: string, maxChars: number): string {
  return text.length <= maxChars ? text : `${text.slice(0, maxChars - 1)}…`
}

/** Technical facts about one uploaded bundle. Nothing here identifies a person. */
export interface ErrorReportIssueInput {
  id: string
  kind: 'user' | 'auto'
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  arch: string
  sizeBytes: number
  r2Key: string
  uploadedUnixSeconds: number
  userNote?: string | null
  email?: string | null
  /**
   * Whether an amendment is what earned this card, rather than the upload. Says so on the card,
   * because an auto-sent report with a card is otherwise a puzzle for whoever triages it.
   */
  filedFromAmendment?: boolean
}

/**
 * The issue for one error report. The note and the reply-to address are deliberately NOT here: they
 * belong in the expiring comment, because this body is kept for as long as the bug is. That holds
 * for the TITLE too, which is why it's technical until the triage agent writes a better one: GitHub
 * keeps every earlier title in the issue's timeline, so a note in it could never be deleted.
 */
export function buildErrorReportIssue(input: ErrorReportIssueInput): IssueContent {
  const kindLine = input.kind === 'user' ? 'hand-written' : 'auto-sent'
  const titleKind = input.filedFromAmendment ? `${kindLine}, note added later` : kindLine
  const uploadedAt = new Date(input.uploadedUnixSeconds * 1000).toISOString().replace('T', ' ').slice(0, 16)

  const body = [
    `**Report id**: \`${input.id}\``,
    `**Kind**: ${kindLine}${input.filedFromAmendment ? ', with a note the reporter added afterwards' : ''}`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)} · **Arch**: ${input.arch}`,
    `**Bundle**: ${formatBytes(input.sizeBytes)}, uploaded ${uploadedAt} UTC`,
    `**R2 key**: \`${input.r2Key}\``,
    '',
    'The note, the reply-to address, and the download link (when there are any) are in the first',
    'comment, which is deleted 90 days after the upload. To fetch the bundle after the link expires,',
    'see `docs/tooling/feedback-and-error-digest.md`.',
  ].join('\n')

  return {
    title: `Error report ${input.id}: ${titleKind} on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body,
    labels: cardLabels('error-report', input.email),
  }
}

/** One crash report, already in D1. Every field here is technical; the reply-to is the only personal one. */
export interface CrashIssueInput {
  rowId: number
  /** `CRASH-XXXXX`, or null for clients older than the field. */
  shortId: string | null
  createdAt: string
  appVersion: string
  osVersion: string
  arch: string
  signal: string
  topFunction: string
  /** `app_fate`: `ended`, `keptRunning`, or null when the row claims nothing. */
  appFate: string | null
  /** Redacted on the user's Mac before it was sent; kept indefinitely like the rest of the row. */
  panicMessage: string | null
  osException: string | null
  osFrames: string[]
  imageBase: string | null
  email?: string | null
}

/** Frames of the macOS stack a crash card shows. The full list is in the D1 row. */
const CRASH_FRAMES_SHOWN = 12

/**
 * The issue for one crash. The row's technical facts go in the body (the privacy policy keeps them
 * indefinitely, and they name nobody); only the reply-to, when there is one, goes in the comment.
 */
export function buildCrashIssue(input: CrashIssueInput): IssueContent {
  const survived = input.appFate === 'keptRunning'
  const fateLine = survived
    ? 'a background panic; the app kept running'
    : input.appFate === 'ended'
      ? 'the app went down'
      : 'unknown (the report says nothing either way)'
  const idPart = input.shortId ? ` ${input.shortId}` : ''
  const what = survived ? 'Panic' : 'Crash'

  const lines = [
    `**Crash id**: \`${input.shortId ?? 'none (older client)'}\` · D1 row \`${String(input.rowId)}\` in \`crash_reports\``,
    `**What happened**: ${fateLine}`,
    `**Where**: \`${input.topFunction}\` · **Signal**: ${input.signal}`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)} · **Arch**: ${input.arch}`,
    `**Reported**: ${input.createdAt.replace('T', ' ').slice(0, 16)} UTC`,
  ]
  if (input.panicMessage) lines.push('', '**Panic message**', '', fencedBlock(input.panicMessage))
  if (input.osException) lines.push('', '**macOS says**', '', fencedBlock(input.osException))
  if (input.osFrames.length) {
    const shown = input.osFrames.slice(0, CRASH_FRAMES_SHOWN)
    const rest = input.osFrames.length - shown.length
    const tail = rest > 0 ? `\n… ${String(rest)} more in the D1 row` : ''
    lines.push('', '**Top of the stack**', '', fencedBlock(shown.join('\n') + tail))
  }
  if (!input.panicMessage && input.imageBase) lines.push('', `**Image base**: \`${input.imageBase}\``)
  lines.push(
    '',
    input.email?.trim()
      ? 'A reply-to address is in the first comment, which is deleted 90 days after the report.'
      : 'No reply-to address was attached.',
  )

  return {
    title: `${what}${idPart} in ${clip(input.topFunction, 80)} (${input.signal}) on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body: truncate(lines.join('\n'), SAFE_TEXT_BUDGET),
    labels: cardLabels('crash-report', input.email),
  }
}

/** One in-app feedback submission, already stored in D1. */
export interface FeedbackIssueInput {
  rowId: number
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  feedback?: string | null
  email?: string | null
}

/**
 * The issue for one feedback message. Unlike an error report, the MESSAGE lives in the body: the
 * privacy policy keeps feedback text so it can be acted on, and only the reply-to address expires.
 * So only the address goes in the expiring comment.
 */
export function buildFeedbackIssue(input: FeedbackIssueInput): IssueContent {
  const message = input.feedback?.trim() ?? ''
  const body = [
    `**Feedback row**: \`${String(input.rowId)}\` in the D1 \`feedback\` table`,
    `**App version**: ${input.appVersion}`,
    `**OS**: ${macOsLabel(input.osVersion)}`,
    '',
    '**What they wrote**',
    '',
    fencedBlock(truncate(message, SAFE_TEXT_BUDGET / 2)),
    '',
    input.email?.trim()
      ? 'A reply-to address is in the first comment, which is deleted two years after it was sent.'
      : 'No reply-to address was attached.',
  ].join('\n')

  return {
    title: `Feedback #${String(input.rowId)} on ${input.appVersion} (${macOsLabel(input.osVersion)})`,
    body: truncate(body, SAFE_TEXT_BUDGET),
    labels: cardLabels('feedback', input.email),
  }
}

/**
 * Whether GitHub says this repo is private, right now.
 *
 * Fails CLOSED in every uncertain case: a network error, a non-200, or a payload without an explicit
 * boolean `private: true` all answer false. This is the gate that keeps a user's note off a public
 * repo, so "we couldn't tell" has to mean "don't write".
 */
export async function isRepoPrivate(target: IssueTarget): Promise<boolean> {
  try {
    const response = await fetch(`${GITHUB_API}/repos/${target.owner}/${target.repo}`, {
      headers: githubHeaders(target.token),
    })
    if (!response.ok) {
      console.error(`GitHub issues: repo probe returned ${String(response.status)}; refusing to write`)
      return false
    }
    const repo: unknown = await response.json()
    // Strictly `true`, never truthy: "private" as a string, or a missing field, must not pass.
    return isRecord(repo) && repo['private'] === true
  } catch (e) {
    console.error('GitHub issues: repo probe failed; refusing to write', e)
    return false
  }
}

/** An issue plus the optional personal half that gets its own, expiring, comment. */
export interface FileIssueInput extends IssueContent {
  personalComment: string | null
}

/**
 * Create one issue, then attach its personal-data comment.
 *
 * Returns the issue number, or null when nothing was created. The privacy probe runs FIRST and a
 * negative answer returns before any write: that ordering is the whole guarantee, so ❌ never reorder
 * these two calls or make the probe conditional.
 *
 * A comment that fails to post still leaves the issue: the technical record is worth keeping, and
 * the note is not lost (it's in the bundle in R2, or the D1 row).
 */
export async function fileIssue(target: IssueTarget, input: FileIssueInput): Promise<number | null> {
  if (!(await isRepoPrivate(target))) {
    console.error(
      `GitHub issues: ${target.owner}/${target.repo} is not private (or could not be verified); nothing was written`,
    )
    return null
  }

  let issueNumber: number
  try {
    const response = await fetch(`${GITHUB_API}/repos/${target.owner}/${target.repo}/issues`, {
      method: 'POST',
      headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
      body: JSON.stringify({ title: input.title, body: input.body, labels: input.labels }),
    })
    if (!response.ok) {
      console.error(`GitHub issues: create failed with ${String(response.status)}`)
      return null
    }
    const created: unknown = await response.json()
    const number = isRecord(created) ? created['number'] : undefined
    if (typeof number !== 'number') {
      console.error('GitHub issues: create returned no issue number')
      return null
    }
    issueNumber = number
  } catch (e) {
    console.error('GitHub issues: create threw', e)
    return null
  }

  if (input.personalComment) {
    try {
      const response = await fetch(
        `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/${String(issueNumber)}/comments`,
        {
          method: 'POST',
          headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
          body: JSON.stringify({ body: input.personalComment }),
        },
      )
      if (!response.ok) {
        console.error(
          `GitHub issues: personal comment failed with ${String(response.status)} on #${String(issueNumber)}`,
        )
      }
    } catch (e) {
      console.error(`GitHub issues: personal comment threw on #${String(issueNumber)}`, e)
    }
  }

  return issueNumber
}

/**
 * Issues one source may file per UTC day.
 *
 * Real volume is a handful, so this never touches legitimate traffic. It exists because `kind` and
 * `buildMode` come from the client's manifest: a buggy or hostile build costs 20 issues a day and
 * then waits. Nothing is lost when it trips: the outbox row stays pending and a later day files it.
 */
export const DAILY_ISSUE_CAP = 20

/** Counted per source so a burst of one kind can never silence another. */
export type IssueSource = 'crash-report' | 'error-report' | 'feedback'

/** Day-scoped keys outlive their day, then expire on their own. Mirrors `error-report-intake.ts`. */
const DAY_KEY_TTL_SECONDS = 48 * 60 * 60

/**
 * Take one slot from a source's daily allowance. Racy like the other KV counters here, which at
 * worst shifts the cutoff by an issue or two.
 */
export async function claimIssueSlot(kv: KVNamespace, source: IssueSource, date: string): Promise<boolean> {
  const key = `gh_issue_count:${source}:${date}`
  const count = parseInt((await kv.get(key)) ?? '0', 10) + 1
  await kv.put(key, String(count), { expirationTtl: DAY_KEY_TTL_SECONDS })
  return count <= DAILY_ISSUE_CAP
}

/** Today in UTC, `YYYY-MM-DD`. */
export function todayUtc(): string {
  return new Date().toISOString().slice(0, 10)
}

/**
 * KV key remembering which issue one report got, so an amendment months later can find its card.
 *
 * Its OWN key rather than a field on the `report:{id}` index: that entry is written before the 200
 * and carries the amend credential's hash, so adding to it would mean a read-modify-write against
 * an eventually-consistent store, with the credential as the thing at risk. A separate key is
 * written once and read once, and a miss simply means no comment.
 */
function issueNumberKey(id: string): string {
  return `gh_issue:${id}`
}

/**
 * Remember an issue number for {@link recallIssueNumber}. TTL matches the report index and the R2
 * lifecycle, so the pointer never outlives the report it points at.
 */
export async function rememberIssueNumber(kv: KVNamespace, id: string, issueNumber: number): Promise<void> {
  await kv.put(issueNumberKey(id), String(issueNumber), { expirationTtl: REPORT_RETENTION_SECONDS })
}

/** The issue one report got, or null when it never got one (auto-send, debug build, cap, outage). */
export async function recallIssueNumber(kv: KVNamespace, id: string): Promise<number | null> {
  const raw = await kv.get(issueNumberKey(id))
  if (raw === null) return null
  const parsed = Number(raw)
  return Number.isInteger(parsed) ? parsed : null
}

/** 90 days, the window a report and everything pointing at it share. */
const REPORT_RETENTION_SECONDS = 90 * 24 * 60 * 60

/** One amendment, as it goes onto the card. */
export interface AmendmentCommentInput {
  note?: string | null
  email?: string | null
  amendmentCount: number
  /** The ORIGINAL report's deletion date, so an amendment can't outlive the bundle it belongs to. */
  expiresOn: string
}

/**
 * The comment one amendment adds to its report's card.
 *
 * Always personal data by definition (an amendment is a note, an address, or both), so it is always
 * stamped and always swept. It carries the original report's date rather than its own: a report
 * amended on day 80 still has to disappear on day 90.
 */
export function buildAmendmentComment(input: AmendmentCommentInput): string {
  const ordinal = `Amendment #${String(input.amendmentCount)}`
  const comment = buildPersonalComment({
    userNote: input.note,
    email: input.email,
    expiresOn: input.expiresOn,
  })
  // `buildPersonalComment` returns null only when there is nothing personal, and the amend route
  // rejects a body carrying neither a note nor an address, so this cannot be empty in practice.
  const parts = comment ?? `${PERSONAL_COMMENT_MARKER} expires=${input.expiresOn} -->`
  return parts.replace('\n\n', `\n\n**${ordinal}**, added by the reporter after sending.\n\n`)
}

/** What one amendment needs to reach the board, and how to describe its report if it has no card. */
export interface AmendmentOnBoardInput {
  id: string
  /** The built amendment comment, already stamped with the original report's expiry date. */
  comment: string
  /** Labels the comment path adds. A filed card gets its labels from the report facts instead. */
  addLabels?: string[]
  /**
   * The report's technical facts, read only when a card has to be created. Null when they can't be
   * read any more, which means no card: a card that can't say what it is about helps nobody.
   */
  readReportFacts: () => Promise<ErrorReportIssueInput | null>
  /** The report's card, when the caller already knows it. Skips the KV lookup. */
  issueNumber?: number
}

/** What happened to one amendment on the board. Every failure is a `skipped`. */
export type AmendmentOnBoardResult = 'commented' | 'filed' | 'skipped'

/**
 * Put one amendment on the board.
 *
 * Usually the report already has a card and the amendment is a comment on it. The outbox passes
 * that card's number when it knows it; otherwise the KV pointer is asked. When the report has no
 * card at all (it predates the outbox, so it never got one), the amendment IS the card: a person
 * typed a sentence about their own report, and that has to reach the board.
 *
 * Runs the privacy probe again through {@link fileIssue}'s own gate rather than trusting that the
 * repo was private when the issue was filed: months can pass between an upload and its amendment,
 * and the answer is allowed to have changed.
 */
export async function recordAmendmentOnBoard(
  env: Bindings,
  input: AmendmentOnBoardInput,
): Promise<AmendmentOnBoardResult> {
  const target = resolveIssueTarget(env)
  if (!target) return 'skipped'

  const issueNumber = input.issueNumber ?? (await recallIssueNumber(env.ERROR_REPORT_META, input.id))
  if (issueNumber === null) {
    return (await fileCardForAmendment(env, target, input)) ? 'filed' : 'skipped'
  }
  return (await commentOnIssue(target, issueNumber, input.comment, input.addLabels ?? [])) ? 'commented' : 'skipped'
}

/**
 * File the card an amended report never got, carrying the amendment as its personal comment.
 *
 * Spends the same daily allowance as the upload path, for the same reason: `kind` and `buildMode`
 * come from the client's manifest. Debug builds stay out, they are our own E2E traffic.
 */
async function fileCardForAmendment(
  env: Bindings,
  target: IssueTarget,
  input: AmendmentOnBoardInput,
): Promise<boolean> {
  const facts = await input.readReportFacts()
  if (!facts) return false
  if (facts.buildMode === 'debug') return false
  if (!(await claimIssueSlot(env.ERROR_REPORT_META, 'error-report', todayUtc()))) {
    console.error('GitHub issues: daily error-report cap reached; the amendment did not get a card')
    return false
  }

  const issueNumber = await fileIssue(target, {
    ...buildErrorReportIssue({ ...facts, filedFromAmendment: true }),
    personalComment: input.comment,
  })
  if (issueNumber === null) return false

  // Remembered so the NEXT amendment comments on this card instead of filing a second one.
  try {
    await rememberIssueNumber(env.ERROR_REPORT_META, input.id, issueNumber)
  } catch (e) {
    console.error('GitHub issues: remembering the issue number failed; a later amendment will file its own card', e)
  }
  return true
}

/** Post one comment on an existing card, then add any labels. False when nothing was posted. */
async function commentOnIssue(
  target: IssueTarget,
  issueNumber: number,
  body: string,
  addLabels: string[],
): Promise<boolean> {
  if (!(await isRepoPrivate(target))) {
    console.error(`GitHub issues: ${target.owner}/${target.repo} is not private; the amendment was not posted`)
    return false
  }

  try {
    const response = await fetch(
      `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/${String(issueNumber)}/comments`,
      {
        method: 'POST',
        headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
        body: JSON.stringify({ body }),
      },
    )
    if (!response.ok) {
      console.error(
        `GitHub issues: amendment comment failed with ${String(response.status)} on #${String(issueNumber)}`,
      )
      return false
    }
  } catch (e) {
    console.error(`GitHub issues: amendment comment threw on #${String(issueNumber)}`, e)
    return false
  }

  if (addLabels.length) {
    await addIssueLabels(target, issueNumber, addLabels)
  }
  return true
}

/**
 * Add labels to an issue, leaving the ones already on it alone: GitHub's add endpoint is additive
 * and idempotent, so re-adding one is a no-op rather than an error.
 *
 * Failure is logged and swallowed. The comment it accompanies is already posted, and losing a label
 * must not read to the caller as losing the amendment.
 */
async function addIssueLabels(target: IssueTarget, issueNumber: number, labels: string[]): Promise<void> {
  try {
    const response = await fetch(
      `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/${String(issueNumber)}/labels`,
      {
        method: 'POST',
        headers: { ...githubHeaders(target.token), 'content-type': 'application/json' },
        body: JSON.stringify({ labels }),
      },
    )
    if (!response.ok) {
      console.error(`GitHub issues: labelling #${String(issueNumber)} returned ${String(response.status)}`)
    }
  } catch (e) {
    console.error(`GitHub issues: labelling #${String(issueNumber)} threw`, e)
  }
}

/**
 * The retention promises the cards' personal comments keep, from
 * `apps/website/src/pages/privacy-policy.astro` § "How long we keep your data". They mirror the
 * constants the D1 retention sweep uses in `scheduled.ts`; change one and the other has to follow.
 * A crash's reply-to shares the error report's 90 days.
 */
export const ERROR_REPORT_RETENTION_DAYS = 90
export const CRASH_EMAIL_RETENTION_DAYS = 90
export const FEEDBACK_EMAIL_RETENTION_DAYS = 730

/** The three fields we read off a GitHub comment, once it has been checked to actually have them. */
interface GitHubComment {
  id: number
  body: string
  issueUrl: string
}

/** Narrow one element of GitHub's comment listing, or null when it isn't shaped as we need. */
function asGitHubComment(raw: unknown): GitHubComment | null {
  if (!raw || typeof raw !== 'object') return null
  const record = raw as Record<string, unknown>
  const id = record['id']
  const body = record['body']
  const issueUrl = record['issue_url']
  if (typeof id !== 'number' || typeof body !== 'string') return null
  return { id, body, issueUrl: typeof issueUrl === 'string' ? issueUrl : '' }
}

/**
 * The issue number at the end of a comment's `issue_url`. Zero when it can't be read, which only
 * costs the log line its issue reference: the comment id is what the delete actually needs.
 */
function issueNumberFromUrl(issueUrl: string): number {
  const last = issueUrl.split('/').pop() ?? ''
  const parsed = Number(last)
  return Number.isFinite(parsed) ? parsed : 0
}

/** One comment the retention sweep may delete, with the date it was promised to go. */
export interface ExpiringComment {
  issueNumber: number
  commentId: number
  expiresOn: string
}

/**
 * Every personal-data comment in the repo that is due for deletion on `today` or earlier.
 *
 * Walks issue comments repo-wide (newest issues carry the youngest stamps, but a 730-day feedback
 * stamp can sit behind 90-day error-report ones, so the walk can't stop early on the first
 * unexpired comment).
 */
export async function listExpiredPersonalComments(target: IssueTarget, today: string): Promise<ExpiringComment[]> {
  const due: ExpiringComment[] = []
  const perPage = 100

  for (let page = 1; page <= 20; page++) {
    const url = `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/comments?per_page=${String(perPage)}&page=${String(page)}`
    const response = await fetch(url, { headers: githubHeaders(target.token) })
    if (!response.ok) {
      throw new Error(`GitHub issues: listing comments returned ${String(response.status)}`)
    }
    const payload: unknown = await response.json()
    const comments: unknown[] = Array.isArray(payload) ? payload : []
    if (comments.length === 0) return due

    for (const raw of comments) {
      const comment = asGitHubComment(raw)
      if (!comment) continue
      const expiresOn = personalCommentExpiry(comment.body)
      if (!expiresOn || expiresOn > today) continue
      due.push({ issueNumber: issueNumberFromUrl(comment.issueUrl), commentId: comment.id, expiresOn })
    }

    if (comments.length < perPage) return due
  }

  return due
}

/** Delete one comment. Throws on failure so the cron's alarm path reports it. */
export async function deleteComment(target: IssueTarget, commentId: number): Promise<void> {
  const response = await fetch(
    `${GITHUB_API}/repos/${target.owner}/${target.repo}/issues/comments/${String(commentId)}`,
    { method: 'DELETE', headers: githubHeaders(target.token) },
  )
  // 404 means it's already gone, which is the state we wanted.
  if (!response.ok && response.status !== 404) {
    throw new Error(`GitHub issues: deleting comment ${String(commentId)} returned ${String(response.status)}`)
  }
}
