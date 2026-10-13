/**
 * ESLint rule: a key chip inside a button goes through `Button`'s `keyHint`
 * prop, never as a `ShortcutChip` placed in the button's content.
 *
 * Rationale: a chip dropped into a button's label flows inline with the text,
 * so it sits on the text's baseline (`vertical-align: middle` is middle of the
 * x-height, not of the button), and it reads low; it also wraps under the
 * label when the button is squeezed (the conflict prompt's "Overwrite all
 * smaller" dropped its `M` to a second line). `Button`'s `keyHint` renders the
 * chip in its own slot, centered on the button's cross axis and never wrapping,
 * hides it from assistive tech, and fills `aria-keyshortcuts` from it. One slot
 * means one look, everywhere a key presses a button.
 *
 * ## What this rule catches
 *
 * A `<ShortcutChip>` anywhere inside a `<Button>` (the house component) or a raw
 * `<button>` element, however deeply wrapped.
 *
 * ## What it deliberately does NOT catch
 *
 * - A chip beside a button, or in prose: that's what `ShortcutChip` is for.
 * - `Button.svelte` itself, which renders the chip in its slot (file-level
 *   opt-out in `eslint.config.js`).
 *
 * A raw `<button>` that genuinely isn't a `Button` (a palette row, the function
 * key bar) opts out per element, with the reason:
 *
 *   <!-- eslint-disable-next-line cmdr/key-hint-via-button-prop -- <reason> -->
 */

/** The nearest enclosing button, the house component or a raw element, or `null`. */
function enclosingButton(node) {
  for (let at = node.parent; at; at = at.parent) {
    if (at.type !== 'SvelteElement') continue
    const name = at.name?.name
    if (at.kind === 'html' && name === 'button') return at
    if (at.kind !== 'html' && name === 'Button') return at
  }
  return null
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: "Put a button's key chip through `Button`'s `keyHint` prop, not inside its content.",
      recommended: true,
    },
    messages: {
      chipInButton:
        'Pass the key as `Button`\'s `keyHint` prop (`keyHint="F2"`, or an object naming a `commandId`) ' +
        "instead of placing a `ShortcutChip` in the button's content. The prop draws the chip in its own slot, " +
        'centered and never wrapping, hides it from screen readers, and fills `aria-keyshortcuts`. See ' +
        "`src/lib/ui/DETAILS.md` § Button. A raw `<button>` that genuinely isn't a `Button` opts out per " +
        'element: `<!-- eslint-disable-next-line cmdr/key-hint-via-button-prop -- <reason> -->`.',
    },
    schema: [],
  },
  create(context) {
    return {
      SvelteElement(node) {
        if (node.kind === 'html' || node.name?.name !== 'ShortcutChip') return
        const button = enclosingButton(node)
        if (!button) return
        // Reported on the BUTTON's start tag, so the opt-out comment sits right above the button.
        context.report({ node: button.startTag, messageId: 'chipInButton' })
      },
    }
  },
}
