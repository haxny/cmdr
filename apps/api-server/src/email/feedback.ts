/**
 * How the three-hour digest (`digest.ts`) renders in-app feedback: one card per message.
 *
 * Cards rather than the crash table, because feedback is prose: a table column shreds a paragraph
 * into a ribbon.
 */

import {
  CARD_FOOTER_STYLE,
  CARD_HEADER_STYLE,
  CARD_PROSE_STYLE,
  CARD_STYLE,
  boardLine,
  escapeHtml,
  replyToLine,
  type BoardCard,
} from './layout'

/**
 * One in-app feedback message, as the digest renders it. Every field here is either written by a
 * person or copied from their machine, so every one of them is escaped at render time.
 */
export interface FeedbackEmailRow {
  /** `created_at`, in the SQLite `datetime('now')` shape (`YYYY-MM-DD HH:MM:SS`, UTC). */
  when: string
  /** `app_version`. */
  version: string
  /** `os_version`. */
  osVersion: string
  /** What the person wrote, verbatim. Untrusted text; line breaks are theirs and are preserved. */
  message: string
  /** The reply-to address the sender chose to attach, or `null` when they attached none. */
  email: string | null
  /** The message's card on the board, or null when the cron hasn't filed it yet. */
  card: BoardCard | null
}

/** One card per message, stacked. */
export function renderFeedbackCard(entry: FeedbackEmailRow): string {
  return `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">
            ${escapeHtml(entry.when)} UTC &middot; app ${escapeHtml(entry.version)} &middot; OS ${escapeHtml(entry.osVersion)}
        </div>
        <div style="${CARD_PROSE_STYLE}">${escapeHtml(entry.message)}</div>
        <div style="${CARD_FOOTER_STYLE}">${replyToLine(entry.email)}</div>
        <div style="${CARD_FOOTER_STYLE}">${boardLine(entry.card)}</div>
    </div>`
}
