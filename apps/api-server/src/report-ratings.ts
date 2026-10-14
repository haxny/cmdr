/**
 * Report card ratings: the cron copies the triage agent's `Rating:` line onto the card's Severity,
 * Impact, and Size fields on the Cmdr backlog board.
 *
 * ## Why the cron and not the agent
 *
 * The triage agent reads stranger-written text, so it holds no board token at all: the only token
 * that can write a user-owned board is a classic `project` PAT, and that one reaches every board the
 * account can, private issue bodies included. The agent ends its one triage comment with a
 * machine-readable line instead, and this job applies it with the Worker's own
 * `GITHUB_PROJECT_TOKEN`. The board fields stay the only home of the values.
 *
 * ## The grammar, exactly
 *
 * The comment's LAST non-blank line, and its only line that starts like a rating:
 *
 *     Rating: Severity=<Critical|High|Medium|Low|none> Impact=<High|Medium|Low> Size=<XS|S|M|L|XL>
 *
 * Fixed order, single spaces, case-sensitive, nothing before or after. `Severity=none` clears the
 * field (cards that aren't bugs). Anything else is rejected, never guessed at.
 *
 * ## Which comments count
 *
 * Only comments that start with `board.sh`'s triage marker AND are authored by
 * `GITHUB_TRIAGE_AUTHOR`, the account the agent's token posts as. That account is David's, and so is
 * the Worker's own issues token, which posts the personal-data comments holding a stranger's note:
 * the marker is what keeps a `Rating:` line typed into a note from counting. Strangers can't comment
 * on the private repo at all.
 *
 * ## Failure is loud, and retried
 *
 * A comment that can't be applied (a malformed line, an unknown option, a card not on the board yet)
 * becomes a `board-rating-failed` ops event in the next digest, at most once a day per comment, and
 * is retried every tick. A failed GitHub call throws, so `runCronJob` raises the alarm.
 */

import { listCommentsUpdatedSince, resolveIssueTarget, type CardComment } from './github-issues'
import { recordOpsEvent } from './ops-events'
import {
  listBoardItemsForRepo,
  readBoardLayout,
  resolveBoardTarget,
  setItemFields,
  type BoardLayout,
  type BoardTarget,
  type ItemFieldValue,
} from './project-board'
import type { Bindings } from './types'

/** The first line `board.sh comment` gives every triage comment. ❗ Must match `board.sh`'s `marker`. */
export const TRIAGE_COMMENT_MARKER = '<!-- cmdr:triage -->'

/** The board's option names, as `docs/tooling/backlog-board.md` defines them. */
export const SEVERITY_OPTIONS = ['Critical', 'High', 'Medium', 'Low'] as const
export const IMPACT_OPTIONS = ['High', 'Medium', 'Low'] as const
export const SIZE_OPTIONS = ['XS', 'S', 'M', 'L', 'XL'] as const

export type Severity = (typeof SEVERITY_OPTIONS)[number]
export type Impact = (typeof IMPACT_OPTIONS)[number]
export type Size = (typeof SIZE_OPTIONS)[number]

export interface CardRating {
  /** Null clears the field: Severity is for bugs and security issues only. */
  severity: Severity | null
  impact: Impact
  size: Size
}

export type RatingParse =
  | { kind: 'absent' }
  | { kind: 'invalid'; reason: string }
  | { kind: 'valid'; rating: CardRating; line: string }

const RATING_LINE = /^Rating: Severity=(\S+) Impact=(\S+) Size=(\S+)$/

/**
 * A line that is trying to be a rating, however mangled (`rating:`, `**Rating**:`, `Rating Size=S`),
 * so a typo is reported rather than read as "no rating at all".
 */
const LOOKS_LIKE_RATING = /^[\W_]*rating\b/i

function isOneOf<T extends string>(options: readonly T[], value: string): value is T {
  return (options as readonly string[]).includes(value)
}

/** Read the `Rating:` line off a triage comment. Strict: see the module comment for the grammar. */
export function parseRatingLine(body: string): RatingParse {
  const lines = body.split(/\r?\n/)
  const ratingLines = lines.filter((line) => LOOKS_LIKE_RATING.test(line))
  if (ratingLines.length === 0) return { kind: 'absent' }
  if (ratingLines.length > 1) {
    return { kind: 'invalid', reason: `it has ${String(ratingLines.length)} Rating lines, and only one is allowed` }
  }

  const line = ratingLines[0] ?? ''
  const lastLine = lines.filter((candidate) => candidate.trim() !== '').pop()
  if (line !== lastLine) return { kind: 'invalid', reason: 'its Rating line has to be the last line' }

  const match = RATING_LINE.exec(line)
  if (!match) {
    return {
      kind: 'invalid',
      reason: `its Rating line isn't exactly "Rating: Severity=<…> Impact=<…> Size=<…>" (fixed order, single spaces)`,
    }
  }
  const [, severity = '', impact = '', size = ''] = match
  if (severity !== 'none' && !isOneOf(SEVERITY_OPTIONS, severity)) {
    return { kind: 'invalid', reason: `Severity "${severity}" isn't one of ${SEVERITY_OPTIONS.join(', ')}, none` }
  }
  if (!isOneOf(IMPACT_OPTIONS, impact)) {
    return { kind: 'invalid', reason: `Impact "${impact}" isn't one of ${IMPACT_OPTIONS.join(', ')}` }
  }
  if (!isOneOf(SIZE_OPTIONS, size)) {
    return { kind: 'invalid', reason: `Size "${size}" isn't one of ${SIZE_OPTIONS.join(', ')}` }
  }
  return { kind: 'valid', rating: { severity: severity === 'none' ? null : severity, impact, size }, line }
}

/**
 * How far back the job looks for created or edited comments. A failure keeps alerting daily inside
 * this window; editing the comment brings it back in.
 */
export const RATING_LOOKBACK_DAYS = 14

/**
 * Triage comments from before the agent wrote a `Rating:` line carry none, and that's not a failure.
 * Older ones are skipped outright.
 */
export const RATING_LINES_SINCE = '2026-10-15T00:00:00Z'

/** Comment pages (of 100) read per tick, and ratings applied per tick; the rest wait a tick. */
const COMMENT_PAGES = 5
export const RATINGS_PER_TICK = 25

interface RatingRow {
  applied_rating: string | null
  last_error: string | null
  last_error_on: string | null
}

/** A comment whose rating is due, with the parse that says what to set. */
interface DueRating {
  comment: CardComment
  rating: CardRating
  line: string
}

/** A comment that couldn't be applied, and why, in words the digest can show. */
interface RatingProblem {
  comment: CardComment
  reason: string
}

/** Every tick: apply each new or changed `Rating:` line. A no-op unless the board, repo, and author are configured. */
export async function applyCardRatings(env: Bindings, now = new Date()): Promise<void> {
  const issues = resolveIssueTarget(env)
  const board = resolveBoardTarget(env)
  const author = env.GITHUB_TRIAGE_AUTHOR
  if (!issues || !board || !author) return

  const db = env.TELEMETRY_DB
  const since = new Date(now.getTime() - RATING_LOOKBACK_DAYS * 86_400_000).toISOString()
  const { comments, truncated } = await listCommentsUpdatedSince(issues, since, COMMENT_PAGES)
  const { due, problems } = await sortComments(db, comments, author)

  let failedWrites = 0
  if (due.length > 0) {
    const layout = await readBoardLayout(board)
    const items = await listBoardItemsForRepo(board, layout, `${issues.owner}/${issues.repo}`)
    for (const entry of due) {
      const outcome = await applyOne(board, layout, items, entry)
      if (outcome === 'applied') await recordApplied(db, entry, now.toISOString())
      else if (outcome === 'refused') failedWrites++
      else problems.push({ comment: entry.comment, reason: outcome.reason })
    }
  }

  for (const { comment, reason } of problems) {
    await recordProblem(db, comment, reason, now.toISOString().slice(0, 10), issues)
  }

  if (failedWrites > 0) {
    throw new Error(
      `${String(failedWrites)} card rating(s) were refused by the board; they retry next tick. Workers logs have the GraphQL response.`,
    )
  }
  if (truncated) {
    throw new Error(
      `More than ${String(COMMENT_PAGES * 100)} comments changed in ${String(RATING_LOOKBACK_DAYS)} days, so the newest weren't read for ratings. Something is flooding the reports repo.`,
    )
  }
}

/**
 * Split the listed comments into ratings still owed to the board (at most {@link RATINGS_PER_TICK})
 * and triage comments that can't be applied. Untrusted and unmarked comments drop out silently:
 * they were never ratings.
 */
async function sortComments(
  db: D1Database,
  comments: CardComment[],
  author: string,
): Promise<{ due: DueRating[]; problems: RatingProblem[] }> {
  const due: DueRating[] = []
  const problems: RatingProblem[] = []
  for (const comment of comments) {
    if (comment.author !== author || !comment.body.startsWith(TRIAGE_COMMENT_MARKER)) continue
    if (comment.createdAt < RATING_LINES_SINCE) continue

    const parsed = parseRatingLine(comment.body)
    if (parsed.kind === 'absent') {
      problems.push({ comment, reason: 'the triage comment has no Rating line' })
    } else if (parsed.kind === 'invalid') {
      problems.push({ comment, reason: `the triage comment's Rating line was rejected: ${parsed.reason}` })
    } else if ((await readRow(db, comment.id))?.applied_rating !== parsed.line && due.length < RATINGS_PER_TICK) {
      due.push({ comment, rating: parsed.rating, line: parsed.line })
    }
  }
  return { due, problems }
}

/** Set one card's fields. `refused` is GitHub saying no, which throws later; a problem is ours to report. */
async function applyOne(
  board: BoardTarget,
  layout: BoardLayout,
  items: Map<number, string>,
  entry: DueRating,
): Promise<'applied' | 'refused' | { reason: string }> {
  const itemId = items.get(entry.comment.issueNumber)
  if (!itemId) return { reason: "the card isn't on the board (yet)" }
  const values = fieldValues(layout, entry.rating)
  if (typeof values === 'string') return { reason: values }
  return (await setItemFields(board, itemId, values)) ? 'applied' : 'refused'
}

/** The three field values for one rating, or why the board can't take it. */
function fieldValues(layout: BoardLayout, rating: CardRating): ItemFieldValue[] | string {
  const wanted: [string, string | null][] = [
    ['Severity', rating.severity],
    ['Impact', rating.impact],
    ['Size', rating.size],
  ]
  const values: ItemFieldValue[] = []
  for (const [name, option] of wanted) {
    const field = layout.fields.get(name)
    if (!field) return `the board has no single-select ${name} field`
    if (option === null) {
      values.push({ fieldId: field.id, optionId: null })
      continue
    }
    const optionId = field.options.get(option)
    if (!optionId) return `the board's ${name} field has no "${option}" option`
    values.push({ fieldId: field.id, optionId })
  }
  return values
}

async function readRow(db: D1Database, commentId: number): Promise<RatingRow | null> {
  return db
    .prepare(`SELECT applied_rating, last_error, last_error_on FROM card_ratings WHERE comment_id = ?`)
    .bind(commentId)
    .first<RatingRow>()
}

async function recordApplied(db: D1Database, entry: DueRating, at: string): Promise<void> {
  await db
    .prepare(
      `INSERT INTO card_ratings (comment_id, issue_number, applied_rating, applied_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(comment_id) DO UPDATE SET applied_rating = excluded.applied_rating,
           applied_at = excluded.applied_at, last_error = NULL, last_error_on = NULL`,
    )
    .bind(entry.comment.id, entry.comment.issueNumber, entry.line, at)
    .run()
  console.log(`Card ratings: #${String(entry.comment.issueNumber)} is now ${entry.line}`)
}

/**
 * Remember why a comment didn't apply, and alert when the reason is new or wasn't alerted today, so
 * a stuck comment reads once a day in the digest rather than every three hours.
 */
async function recordProblem(
  db: D1Database,
  comment: CardComment,
  reason: string,
  today: string,
  issues: { owner: string; repo: string },
): Promise<void> {
  const row = await readRow(db, comment.id)
  await db
    .prepare(
      `INSERT INTO card_ratings (comment_id, issue_number, last_error, last_error_on) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(comment_id) DO UPDATE SET last_error = excluded.last_error, last_error_on = excluded.last_error_on`,
    )
    .bind(comment.id, comment.issueNumber, reason, today)
    .run()
  if (row?.last_error === reason && row.last_error_on === today) return

  const link = `https://github.com/${issues.owner}/${issues.repo}/issues/${String(comment.issueNumber)}#issuecomment-${String(comment.id)}`
  await recordOpsEvent(
    db,
    'board-rating-failed',
    `Card #${String(comment.issueNumber)} didn't get its Severity, Impact, and Size: ${reason}. Triage comment: ${link}. Fix its last line to "Rating: Severity=<…> Impact=<…> Size=<…>" (an edit re-applies), or rate the card by hand. It retries every tick and alerts daily until ${String(RATING_LOOKBACK_DAYS)} days after the comment's last edit.`,
  )
}
