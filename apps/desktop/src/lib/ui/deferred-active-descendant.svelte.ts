/**
 * `aria-activedescendant` that lands one frame AFTER its element takes focus, for a composite
 * widget (a menu surface, a file list) whose focused element names its cursor row.
 *
 * ❗ VoiceOver reads only the container ("menu Volume switcher") when focus and the attribute
 * arrive together, and reads the row ("Macintosh HD APFS current location menu item, …") when
 * the attribute follows a frame later (macOS 27.0, VoiceOver 10, Cmdr 0.52.0 dev, real keys
 * through the AppleScript bridge, 2026-10-15). So the attribute stays off until the element
 * has focus and a frame has passed. A cursor move after focus shows at once: that's a change
 * of its own, which VoiceOver reads like any ↓.
 */

/**
 * The frame fallback, in ms. WKWebView starves animation frames in a window without focus
 * (`docs/testing.md` § "`requestAnimationFrame` in unfocused windows"), and an id that never
 * lands would leave the row unnamed for good.
 */
const FRAME_FALLBACK_MS = 100

/** Runs `run` once, on the next animation frame or after the fallback, whichever comes first. Returns a cancel. */
export function afterNextFrame(run: () => void): () => void {
  let done = false
  const once = (): void => {
    if (done) return
    done = true
    cancelAnimationFrame(frame)
    clearTimeout(timer)
    run()
  }
  const frame = requestAnimationFrame(once)
  const timer = setTimeout(once, FRAME_FALLBACK_MS)
  return () => {
    done = true
    cancelAnimationFrame(frame)
    clearTimeout(timer)
  }
}

export interface DeferredActiveDescendant {
  /** What to render as `aria-activedescendant` while the cursor row's id is `id`. */
  of: (id: string | undefined) => string | undefined
  /** Focuses `el` without scrolling and holds `id` (the cursor's id right now) back until the next frame. */
  focus: (el: HTMLElement, id: string | undefined) => void
  /**
   * The element took focus by some other path (a click, a `.focus()` elsewhere), reported
   * from its own `focus` event: hold `id` back until the next frame, as `focus` does.
   */
  tookFocus: (id: string | undefined) => void
  /**
   * The cursor row's node was replaced under an unchanged id (entering a folder re-renders
   * row 0): WebKit's focus stays on the dead node and VoiceOver says nothing, so hold `id`
   * back for a frame again and let it land as a change. No-op while the element lacks focus.
   */
  renew: (id: string) => void
  /** The element lost its role (unmounted, or focus left it for good): hold again until its next `focus`. */
  reset: () => void
}

export function createDeferredActiveDescendant(): DeferredActiveDescendant {
  let released = $state(false)
  let focused = $state(false)
  let heldId = $state<string | undefined>(undefined)
  let cancel: (() => void) | null = null

  function stop(): void {
    cancel?.()
    cancel = null
  }

  function hold(id: string | undefined): void {
    stop()
    released = false
    focused = true
    heldId = id
    cancel = afterNextFrame(() => {
      cancel = null
      released = true
    })
  }

  return {
    of(id) {
      if (released) return id
      return focused && id !== heldId ? id : undefined
    },
    focus(el, id) {
      hold(id)
      el.focus({ preventScroll: true })
    },
    tookFocus: hold,
    renew(id) {
      if (focused) hold(id)
    },
    reset() {
      stop()
      released = false
      focused = false
      heldId = undefined
    },
  }
}
