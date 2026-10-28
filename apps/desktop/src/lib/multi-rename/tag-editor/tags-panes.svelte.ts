/**
 * The Tags window's two panes, Album | Tracks, as the main window's two file panes: which one has focus, each one's
 * cursor, and Tab's switch (`tags.switchPane`). The DOM focus is the caller's (`focus`), so this stays testable.
 */

export type TagsPaneId = 'album' | 'tracks'

export interface TagsPanes {
  /** The pane focus is in, or was in while focus visits the tools bar; `null` before either had it, or after it left both. */
  readonly active: TagsPaneId | null
  albumCursor: number
  tracksCursor: number
  /** Records where focus went (`null`: somewhere that is neither pane nor the tools bar). */
  noteFocus(pane: TagsPaneId | null): void
  /** Focuses the other pane (the album when none was active) and returns it. */
  switchPane(): TagsPaneId
}

export function createTagsPanes(deps: { focus: (pane: TagsPaneId) => void }): TagsPanes {
  let active = $state<TagsPaneId | null>(null)
  let albumCursor = $state(0)
  let tracksCursor = $state(0)

  return {
    get active() {
      return active
    },
    get albumCursor() {
      return albumCursor
    },
    set albumCursor(index: number) {
      albumCursor = index
    },
    get tracksCursor() {
      return tracksCursor
    },
    set tracksCursor(index: number) {
      tracksCursor = index
    },
    noteFocus(pane) {
      active = pane
    },
    switchPane() {
      const next: TagsPaneId = active === 'album' ? 'tracks' : 'album'
      active = next
      deps.focus(next)
      return next
    },
  }
}
