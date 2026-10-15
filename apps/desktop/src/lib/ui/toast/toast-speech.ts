/**
 * What a screen reader says for a toast: its text, read off the rendered body so a
 * component toast speaks the same words a string toast does.
 *
 * Controls stay out. Focus reaches each one and it names itself there, so reading
 * "Go to trash Undo" after the message would only make every trash notice longer.
 * Anything `aria-hidden` (the age label) stays out too, as it does for the reader.
 */

/** What a toast says once it's posted (and again on each re-raise), for the stack's live regions. */
export interface ToastSpeech {
  text: string
  /** Warnings and failures interrupt; everything else waits its turn. */
  urgent: boolean
}

const SKIPPED = 'button, a[href], input, select, textarea, [role="button"], [aria-hidden="true"]'

/** The toast body's spoken text, whitespace collapsed. */
export function toastSpeech(body: Element): string {
  const parts: string[] = []
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      node instanceof Element && node.matches(SKIPPED) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
  })
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node.nodeType === Node.TEXT_NODE) parts.push(node.textContent ?? '')
    else if (isBlock(node)) parts.push(' ')
  }
  return parts.join('').replace(/\s+/g, ' ').trim()
}

/** A row boundary, where two runs of text need a space between them. */
function isBlock(node: Node): boolean {
  return node instanceof HTMLElement && /^(P|DIV|LI|BR)$/.test(node.tagName)
}
