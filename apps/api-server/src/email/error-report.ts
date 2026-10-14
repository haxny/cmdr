/**
 * How the three-hour digest (`digest.ts`) renders error reports and the amendments people add to
 * them afterwards. Cards, sharing the chrome with feedback, so a person writing to us looks the same
 * in the inbox whichever surface they used.
 */

import { formatBytes } from '../types'
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
 * One error report, as the digest renders it. Everything here either comes off the reporter's
 * machine or was typed by them, so every field is escaped at render time.
 */
export interface ErrorReportEmailRow {
  /** The `ERR-XXXXX` the person read in the send dialog, so a reply can name the same id. */
  id: string
  kind: 'user' | 'auto'
  appVersion: string
  osVersion: string
  arch: string
  /** Bundle size in bytes, rendered through {@link formatBytes}. */
  sizeBytes: number
  /** What the person wrote, or null for an auto-send or a report sent without a note. */
  userNote: string | null
  /** Reply-to address, present only when the reporter ticked "Attach my email" in the send dialog. */
  email: string | null
  /** Presigned R2 GET URL, or `null` when the credentials to mint one aren't configured. */
  downloadUrl: string | null
  /** How long {@link downloadUrl} keeps working, so a stale click isn't a mystery. */
  linkTtlHours: number
  /** The report's card on the board, or null when the cron hasn't filed it yet. */
  card: BoardCard | null
}

/** The prose block: what they wrote, or a plain line saying there wasn't anything. */
function noteBlock(note: string | null | undefined, emptyLine: string): string {
  const trimmed = note?.trim()
  return trimmed
    ? `<div style="${CARD_PROSE_STYLE}">${escapeHtml(trimmed)}</div>`
    : `<div style="${CARD_PROSE_STYLE} color: #6b7280;">${escapeHtml(emptyLine)}</div>`
}

/** One report: the machine facts, the note, who to reply to, the bundle, and its card. */
export function renderErrorReportCard(report: ErrorReportEmailRow): string {
  const hours = report.linkTtlHours.toString()
  const downloadLine = report.downloadUrl
    ? `<a href="${escapeHtml(report.downloadUrl)}" style="color: #2563eb;">Download the bundle</a> &middot; the link works for ${hours} hours`
    : 'No download link this time. Fetch the bundle through the Cloudflare API.'
  const kindLine = report.kind === 'user' ? 'hand-written' : 'auto-sent'
  const emptyLine = report.kind === 'user' ? 'No note came with this one.' : 'Auto-sent, so no note.'

  return `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">
            ${escapeHtml(report.id)} &middot; ${kindLine} &middot; app ${escapeHtml(report.appVersion)} &middot; OS ${escapeHtml(report.osVersion)} &middot; ${escapeHtml(report.arch)} &middot; ${escapeHtml(formatBytes(report.sizeBytes))}
        </div>
        ${noteBlock(report.userNote, emptyLine)}
        <div style="${CARD_FOOTER_STYLE}">${replyToLine(report.email)}</div>
        <div style="${CARD_FOOTER_STYLE}">${downloadLine}</div>
        <div style="${CARD_FOOTER_STYLE}">${boardLine(report.card)}</div>
    </div>`
}

/** One amendment someone added to a report that was already sent. */
export interface ErrorReportAmendmentRow {
  /** The `ERR-XXXXX` of the report this lands on, so the reply can name the same id. */
  id: string
  /** What they added. Null when they only wanted to leave an address. */
  note: string | null
  /** A reply-to address added after the fact. Null when they only wanted to add a note. */
  email: string | null
  /** How many amendments this report now carries, so a third one reads as a thread, not a repeat. */
  amendmentCount: number
  /** The amended report's card, or null when it has none yet. */
  card: BoardCard | null
}

/**
 * One amendment card. The original bundle is not re-linked here: the id and the card are enough to
 * find it, and a second presigned link per note would multiply live download URLs for one bundle.
 */
export function renderAmendmentCard(amendment: ErrorReportAmendmentRow): string {
  const countLine =
    amendment.amendmentCount > 1
      ? ` &middot; amendment ${escapeHtml(amendment.amendmentCount.toString())} on this report`
      : ''

  return `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">${escapeHtml(amendment.id)}${countLine}</div>
        ${noteBlock(amendment.note, 'No note this time, only the address.')}
        <div style="${CARD_FOOTER_STYLE}">${replyToLine(amendment.email)}</div>
        <div style="${CARD_FOOTER_STYLE}">${boardLine(amendment.card)}</div>
    </div>`
}
