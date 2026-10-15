/**
 * What a screen reader says for a toast: its text, read off the rendered body so a
 * component toast speaks the same words a string toast does.
 *
 * Controls stay out. Focus reaches each one and it names itself there, so reading
 * "Go to trash Undo" after the message would only make every trash notice longer.
 * A control inside a sentence is the exception: "configurable in <Settings > Keyboard
 * shortcuts>." left without its link says "configurable in ." Anything `aria-hidden`
 * (the age label) stays out, as it does for the reader.
 */

/** What a toast says once it's posted (and again on each re-raise), for the stack's live regions. */
export interface ToastSpeech {
  text: string
  /** Warnings and failures interrupt; everything else waits its turn. */
  urgent: boolean
}

const SKIPPED = 'input, select, textarea, [aria-hidden="true"]'
const CONTROL = 'button, a[href], [role="button"]'

/** The toast body's spoken text, whitespace collapsed. */
export function toastSpeech(body: Element): string {
  const parts: string[] = []
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => {
      if (!(node instanceof Element)) return NodeFilter.FILTER_ACCEPT
      if (node.matches(SKIPPED)) return NodeFilter.FILTER_REJECT
      if (node.matches(CONTROL) && !isInSentence(node, body)) return NodeFilter.FILTER_REJECT
      return NodeFilter.FILTER_ACCEPT
    },
  })
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node.nodeType === Node.TEXT_NODE) parts.push(node.textContent ?? '')
    else if (isBlock(node)) parts.push(' ')
  }
  return parts.join('').replace(/\s+/g, ' ').trim()
}

/**
 * True when `control` sits in running text: its row has words of its own (not in a nested
 * row) outside any control. An action row ("Go to trash", "Undo") has none, so its buttons
 * stay out.
 */
function isInSentence(control: Element, body: Element): boolean {
  const row = rowOf(control, body)
  const texts = document.createTreeWalker(row, NodeFilter.SHOW_TEXT)
  for (let text = texts.nextNode(); text; text = texts.nextNode()) {
    const parent = text.parentElement
    if (!parent || !text.textContent?.trim() || parent.closest(`${CONTROL}, ${SKIPPED}`)) continue
    if (rowOf(text, body) === row) return true
  }
  return false
}

/** The nearest row (block) holding `node`, or `body` when none sits between them. */
function rowOf(node: Node, body: Element): Element {
  for (let element = node.parentElement; element && element !== body; element = element.parentElement) {
    if (isBlock(element)) return element
  }
  return body
}

/** A row boundary, where two runs of text need a space between them. */
function isBlock(node: Node): boolean {
  return node instanceof HTMLElement && /^(P|DIV|LI|BR)$/.test(node.tagName)
}
