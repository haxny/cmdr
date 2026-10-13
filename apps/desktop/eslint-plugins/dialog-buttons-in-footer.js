/**
 * ESLint rule: a `ModalDialog`'s action buttons go in its `footer` snippet, never
 * in a button row hand-built in the body.
 *
 * Rationale: `ModalDialog`'s footer guarantees its button row never wraps, in any
 * language, by raising the panel's `min-width` to the row's measured width. A row
 * built in the body gets none of that: under a fixed `width` / `max-width`, a
 * longer translation drops a button onto a second line or wraps its label (French's
 * "Revenir en arrière" did it to the progress dialog's Rollback). The footer also
 * owns the right-alignment, the gap, and the bottom inset, so a body row repeats
 * CSS the dialog already has. See `src/lib/ui/DETAILS.md` § ModalDialog.
 *
 * ## What this rule catches
 *
 * Inside a `<ModalDialog>`'s body (its default children, not a snippet prop like
 * `footer` or `footerLeading`), any element, or the body itself, holding two or
 * more `<Button>`s as direct children. `{#if}` / `{:else}` / `{#each}` / `{#key}`
 * branches count as direct: a row whose buttons swap by state is still one row.
 *
 * ## What it deliberately does NOT catch
 *
 * - A single in-body `Button` beside content (a Try again next to a status line).
 * - `size="mini"` buttons: an in-content toolbar (Allow all / Deny all, jump links).
 * - A raw `<button>` (a link-style toggle, a copy affordance): those aren't actions.
 * - A layout the footer can't express (a grid of choices, multiple labelled rows),
 *   which opts out per element with the reason, and must keep itself from wrapping:
 *
 *   <!-- eslint-disable-next-line cmdr/dialog-buttons-in-footer -- <reason> -->
 */

/** A `<Button>` (the house component), as opposed to a raw `<button>` element. */
function isButton(node) {
  return node.type === 'SvelteElement' && node.kind !== 'html' && node.name?.name === 'Button'
}

/** `size="mini"`: an in-content toolbar control (Allow all, a jump link), never a dialog action. */
function isMini(node) {
  return node.startTag.attributes.some(
    (a) =>
      a.type === 'SvelteAttribute' &&
      a.key?.name === 'size' &&
      a.value.length === 1 &&
      a.value[0].type === 'SvelteLiteral' &&
      a.value[0].value === 'mini',
  )
}

/** A button that would belong in a footer: a full-size `Button`. */
function isActionButton(node) {
  return isButton(node) && !isMini(node)
}

/** Children of a block or element, seeing through control-flow blocks as if their branches were inline. */
function flattenedChildren(children) {
  const out = []
  for (const child of children ?? []) {
    switch (child.type) {
      case 'SvelteIfBlock':
      case 'SvelteEachBlock':
        out.push(...flattenedChildren(child.children))
        if (child.else) out.push(...flattenedChildren(child.else.children))
        break
      case 'SvelteKeyBlock':
        out.push(...flattenedChildren(child.children))
        break
      default:
        out.push(child)
    }
  }
  return out
}

/** Is this node in a `ModalDialog`'s body, rather than one of its snippet props or outside it? */
function inModalDialogBody(node) {
  for (let at = node.parent; at; at = at.parent) {
    // A snippet declared inside the dialog is a prop (`footer`, `title`, ...), not its body.
    if (at.type === 'SvelteSnippetBlock') return false
    if (at.type === 'SvelteElement' && at.kind !== 'html' && at.name?.name === 'ModalDialog') return true
  }
  return false
}

function isModalDialog(node) {
  return node.kind !== 'html' && node.name?.name === 'ModalDialog'
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: "Put a ModalDialog's action buttons in its `footer` snippet, not a button row in its body.",
      recommended: true,
    },
    messages: {
      buttonRowInBody:
        "Move these buttons into `ModalDialog`'s `footer` snippet (and a left-side control into `footerLeading`). " +
        'The footer keeps its button row on one line in every language by widening the dialog; a row built in the ' +
        "body wraps under a long translation. See `src/lib/ui/DETAILS.md` § ModalDialog. A layout the footer can't " +
        'express opts out per element: `<!-- eslint-disable-next-line cmdr/dialog-buttons-in-footer -- <reason> -->`.',
    },
    schema: [],
  },
  create(context) {
    return {
      SvelteElement(node) {
        if (isButton(node)) return
        const isBody = isModalDialog(node)
        if (!isBody && !inModalDialogBody(node)) return
        const buttons = flattenedChildren(node.children).filter(isActionButton)
        if (buttons.length < 2) return
        // On the row's start tag (the first button's, for the body itself), so the
        // opt-out comment sits right above what it excuses.
        context.report({ node: isBody ? buttons[0].startTag : node.startTag, messageId: 'buttonRowInBody' })
      },
    }
  },
}
