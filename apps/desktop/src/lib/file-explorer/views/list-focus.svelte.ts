/**
 * DOM focus and `aria-activedescendant` for a file list (`BriefList`, `FullList`).
 *
 * The focused pane's listbox holds DOM focus and names its cursor row through
 * `aria-activedescendant`; focus never sits on a row (rows unmount under virtualization, and
 * focus on a removed node drops to `<body>`). The id lands one frame after the listbox takes
 * focus, and again one frame after the cursor row's node is replaced under it, both through
 * `#lib/ui/deferred-active-descendant.svelte.ts`, where the VoiceOver evidence lives.
 *
 * The listbox reports focus from its own `focus` / `blur` events, so every path in counts:
 * `focusList`, a click (the browser's mousedown default), the explorer's focus guard.
 */

import { untrack } from 'svelte'
import { createDeferredActiveDescendant } from '#lib/ui/deferred-active-descendant.svelte.js'

/** The rendered cursor row: its document-unique id, and the key of the node behind it (its path). */
export interface CursorRowRef {
  id: string
  key: string
}

export interface ListFocus {
  /** The listbox's `aria-activedescendant`: the rendered cursor row, while the listbox has focus. */
  activeDescendant: () => string | undefined
  /** Focuses the listbox without scrolling (it's taller than its scroller); false when it isn't mounted. */
  focus: (listbox: HTMLElement | undefined) => boolean
  /** Bind to the listbox's `onfocus`. */
  handleFocus: () => void
  /** Bind to the listbox's `onblur`. */
  handleBlur: () => void
}

/**
 * Call during component init (it installs an effect). `getCursorRow` reads the rendered
 * cursor row, `undefined` when the cursor names nothing in the virtual window.
 */
export function createListFocus(getCursorRow: () => CursorRowRef | undefined): ListFocus {
  const descendant = createDeferredActiveDescendant()
  let last: CursorRowRef | undefined

  // ❗ A navigation re-renders the cursor row as a NEW node under the same id (row 0 in
  // both folders), and WebKit keeps its focus on the dead one: VoiceOver stays silent on
  // entering a folder. So when the row appears, or its node changes under an unchanged id,
  // the id lands again a frame later. A plain cursor move changes the id and shows at once.
  $effect(() => {
    const row = getCursorRow()
    untrack(() => {
      if (row && (!last || (last.id === row.id && last.key !== row.key))) descendant.renew(row.id)
      last = row
    })
  })

  return {
    activeDescendant: () => descendant.of(getCursorRow()?.id),
    focus(listbox) {
      if (!listbox?.isConnected) return false
      listbox.focus({ preventScroll: true })
      return true
    },
    handleFocus() {
      descendant.tookFocus(getCursorRow()?.id)
    },
    handleBlur() {
      descendant.reset()
    },
  }
}
