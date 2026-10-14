/**
 * The HTML vocabulary every notification email is built from: escaping, the page shell, the card
 * chrome, and the table cell styles.
 *
 * Everything is inline style with explicit hex. A mail client is not a browser: `<style>` blocks
 * are stripped by some, and `prefers-color-scheme` support is a coin flip, so these pages commit
 * to one light palette rather than rendering as black-on-black somewhere.
 */

const htmlEscapeMap: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }

/**
 * Escape text for HTML. Every value these emails render is either typed by a person or copied off
 * their machine, so it all goes through here.
 */
export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (char) => htmlEscapeMap[char])
}

const FONT_STACK = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"

/**
 * The `<body>` chrome, parameterized because the two families of email don't look alike yet: the
 * report digest sits on an explicit white ground at `#1f2937` text, while the ops alerts (DB size,
 * device count) are `#333` on the client's default ground. Unifying them changes what lands in a
 * human inbox, so it waits for a copy-and-design pass rather than riding along with a refactor.
 */
export function bodyStyle(options: { maxWidthPx: number; color: string; background?: string }): string {
  const background = options.background === undefined ? '' : ` background: ${options.background};`
  return `font-family: ${FONT_STACK}; line-height: 1.6; color: ${options.color}; max-width: ${String(options.maxWidthPx)}px; margin: 0 auto; padding: 20px;${background}`
}

/** The `<body>` chrome the card emails share. */
export const CARD_BODY_STYLE = bodyStyle({ maxWidthPx: 680, color: '#1f2937', background: '#ffffff' })

/** A card's shared chrome: the rounded border and the white ground the header and footer sit on. */
export const CARD_STYLE = 'border: 1px solid #e5e7eb; border-radius: 8px; margin: 0 0 20px; background: #ffffff;'

/** The muted strip at the top of a card, carrying the machine facts. */
export const CARD_HEADER_STYLE =
  'padding: 10px 16px; background: #f9fafb; border-bottom: 1px solid #e5e7eb; border-radius: 8px 8px 0 0; font-size: 13px; color: #6b7280;'

/** The strip at the bottom of a card, carrying the follow-up action. */
export const CARD_FOOTER_STYLE = 'padding: 10px 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;'

/** Prose a person wrote: a readable measure, and their line breaks kept. */
export const CARD_PROSE_STYLE =
  'padding: 16px; max-width: 600px; font-size: 15px; line-height: 1.6; color: #1f2937; white-space: pre-wrap; word-break: break-word;'

/** The closing line under the content, explaining who sent this and why. */
export const SIGNOFF_STYLE =
  'margin-top: 24px; padding-top: 16px; border-top: 1px solid #e5e7eb; font-size: 13px; color: #6b7280;'

/** The grid a table alert lays its rows on. */
export const TABLE_STYLE = 'border-collapse: collapse; width: 100%; margin: 16px 0;'

/** Every table cell's base: the padding and the hairline border. Variants append to it. */
export const CELL_STYLE = 'padding: 8px 12px; border: 1px solid #e5e7eb;'

/** A header cell, aligned per column. */
export function headCellStyle(align: 'left' | 'center' | 'right' = 'left'): string {
  return `${CELL_STYLE} text-align: ${align}; background: #f9fafb;`
}

/**
 * The document every email except the license mail is wrapped in. The license mail carries a
 * `<style>` block and class names of its own, so it builds its own page.
 */
export function documentShell(body: string, style: string): string {
  return `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
</head>
<body style="${style}">
${body}
</body>
</html>
  `.trim()
}

/** The one line at the bottom saying what sent this and why. */
export function signoffParagraph(text: string): string {
  return `<p style="${SIGNOFF_STYLE}">
        ${escapeHtml(text)}
    </p>`
}

/**
 * A card footer's reply line: the address as a `mailto:`, or a plain statement that there isn't
 * one. Every channel a person can attach an address to (feedback, error reports, amendments) says
 * it the same way, and the address stays clickable even when the message's `Reply-To` header
 * can't carry it (a digest of several messages).
 */
export function replyToLine(email: string | null | undefined): string {
  if (!email) return 'No reply-to address'
  return `Reply to <a href="mailto:${escapeHtml(email)}" style="color: #2563eb;">${escapeHtml(email)}</a>`
}

/** A report's card on the private reports board. */
export interface BoardCard {
  number: number
  url: string
}

/**
 * The line every digest entry ends on: a link to its card, or a plain statement that the cron hasn't
 * filed one yet. The board is where a report waits to be dealt with, so the email always points there.
 */
export function boardLine(card: BoardCard | null): string {
  if (!card) return 'Not on the board yet; the next run files it.'
  return `On the board: <a href="${escapeHtml(card.url)}" style="color: #2563eb;">#${escapeHtml(String(card.number))}</a>`
}
