/**
 * Who owns a right-click, in EVERY Cmdr window: the text field under the pointer,
 * or Cmdr. WebKit's own page menu ("Reload", plus "Inspect Element" in a dev
 * build) must never show anywhere in the app.
 *
 * The root layout installs this once per webview (`routes/+layout.svelte`), so a
 * new window route can't come up without it; `routes/window-route-coverage.test.ts`
 * guards that. The decision is a pure function so it's unit-testable.
 *
 * Cmdr owns right-click almost everywhere: the file rows, tabs, breadcrumb,
 * volume rows, query results, network rows, and queue rows each open a native
 * macOS menu of their own, and a surface with no menu should show nothing (two
 * menus for one click, or a "Reload" that wipes the window's state, otherwise).
 *
 * Text fields are the exception: WebKit's own editing menu (Cut / Copy / Paste /
 * Select All, plus the system Services and spelling entries) is exactly the menu
 * a text field should have, and it acts on the field directly, so it can't
 * double up with the `edit.*` command handlers the way ⌘V once did.
 */
import { isTextInputTarget } from '#lib/utils/text-input-focus.js'

/** What the document-level listener does with the right-click. */
export type GlobalContextMenuAction =
  /**
   * Let WebKit open its text-editing menu, and stop the event before any ancestor
   * handler opens a Cmdr menu instead (the inline rename editor and the volume
   * switcher's favorite-rename field sit inside rows that have their own menu).
   */
  | 'native-text-menu'
  /** `preventDefault`, so the only right-click menus outside text fields are Cmdr's. */
  | 'suppress'

/** Decides who owns this right-click: the text field under the pointer, or Cmdr. */
export function resolveGlobalContextMenuAction(event: MouseEvent): GlobalContextMenuAction {
  return isTextInputTarget(event.target) ? 'native-text-menu' : 'suppress'
}

function handleContextMenu(event: MouseEvent): void {
  if (resolveGlobalContextMenuAction(event) === 'native-text-menu') {
    event.stopPropagation()
    return
  }
  event.preventDefault()
}

/**
 * Installs the window's right-click owner and returns its teardown.
 *
 * Registered in the CAPTURE phase so a text field can claim the click before the
 * row / tab / chip handler underneath it opens a Cmdr menu. Outside text fields it
 * only `preventDefault`s and lets the event keep bubbling, so those handlers still
 * open their menus, and they needn't `preventDefault` again themselves.
 */
export function installContextMenuOwnership(): () => void {
  document.addEventListener('contextmenu', handleContextMenu, true)
  return () => {
    document.removeEventListener('contextmenu', handleContextMenu, true)
  }
}
