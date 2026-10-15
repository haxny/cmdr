import { describe, it, expect, vi, afterEach } from 'vitest'
import { captureFocusOrigin, focusHome, isFocusAdrift, registerFocusHome, restoreFocus } from './focus-restore'

const menuFocus = vi.hoisted(() => ({ inOpenMenu: false }))
vi.mock('./menu-controller.svelte', () => ({ isFocusInOpenMenu: () => menuFocus.inOpenMenu }))

function focusable(parent: HTMLElement = document.body, tag = 'div'): HTMLElement {
  const el = document.createElement(tag)
  el.tabIndex = -1
  parent.appendChild(el)
  return el
}

/** An explorer-like home: a root holding a list, whose refocus lands on the list. */
function mountHome() {
  const root = focusable()
  const list = focusable(root)
  const refocus = vi.fn(() => {
    list.focus()
  })
  const unregister = registerFocusHome({ root: () => root, refocus })
  return { root, list, refocus, unregister }
}

let cleanup: (() => void) | null = null

afterEach(() => {
  cleanup?.()
  cleanup = null
  menuFocus.inOpenMenu = false
  document.body.innerHTML = ''
})

describe('restoreFocus', () => {
  it('sends focus that started in the home through its refocus, never a bare focus on the old element', () => {
    const home = mountHome()
    cleanup = home.unregister
    home.list.focus()
    const origin = captureFocusOrigin()
    const bareFocus = vi.spyOn(home.list, 'focus')
    focusable().focus()

    restoreFocus(origin)

    expect(home.refocus).toHaveBeenCalledTimes(1)
    // The refocus spy itself focuses the list; what matters is no `.focus()` before it.
    expect(bareFocus).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(home.list)
  })

  // The remembered list can unmount while the overlay is up (a view toggle behind it).
  it('still refocuses the home when the element remembered there has gone', () => {
    const home = mountHome()
    cleanup = home.unregister
    home.list.focus()
    const origin = captureFocusOrigin()
    home.list.remove()

    restoreFocus(origin)

    expect(home.refocus).toHaveBeenCalledTimes(1)
  })

  it('focuses an element outside the home directly, and skips one that left the document', () => {
    const home = mountHome()
    cleanup = home.unregister
    const outside = focusable(document.body, 'button')
    outside.focus()
    const origin = captureFocusOrigin()
    home.list.focus()

    restoreFocus(origin)
    expect(document.activeElement).toBe(outside)
    expect(home.refocus).not.toHaveBeenCalled()

    home.list.focus()
    outside.remove()
    restoreFocus(origin)
    expect(document.activeElement).toBe(home.list)
  })

  // ⌥F1 then Tab: the chooser re-opens in the other pane, and the closing one's restorer
  // must not pull focus off it (focus stays on the moved chooser today).
  it('leaves focus alone when it already sits in another open menu', () => {
    const home = mountHome()
    cleanup = home.unregister
    home.list.focus()
    const origin = captureFocusOrigin()
    focusable().focus()
    menuFocus.inOpenMenu = true

    restoreFocus(origin)

    expect(home.refocus).not.toHaveBeenCalled()
  })

  it('leaves focus alone in another open dialog, but not in the one closing', () => {
    const home = mountHome()
    cleanup = home.unregister
    home.list.focus()
    const origin = captureFocusOrigin()
    const dialog = focusable()
    dialog.setAttribute('role', 'dialog')
    const field = focusable(dialog, 'input')
    field.focus()

    restoreFocus(origin)
    expect(home.refocus).not.toHaveBeenCalled()

    restoreFocus(origin, { leaving: dialog })
    expect(home.refocus).toHaveBeenCalledTimes(1)
  })

  // The palette running "Move focus to the pane controls": the command focuses a pane
  // control, then the palette unmounts and restores. Sending focus home would undo the command.
  it('leaves focus on a home element the home says keeps it, and only there', () => {
    const root = focusable()
    const list = focusable(root)
    const control = focusable(root, 'button')
    const refocus = vi.fn(() => {
      list.focus()
    })
    cleanup = registerFocusHome({ root: () => root, refocus, keepsFocus: (el) => el === control })
    list.focus()
    const origin = captureFocusOrigin()
    const palette = focusable()

    control.focus()
    restoreFocus(origin, { leaving: palette })
    expect(refocus).not.toHaveBeenCalled()
    expect(document.activeElement).toBe(control)

    palette.focus()
    restoreFocus(origin, { leaving: palette })
    expect(refocus).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(list)
  })

  // The palette running "Move focus to the notifications": the command focuses the newest
  // toast, then the palette unmounts and restores. Sending focus home would undo the command.
  it('leaves focus in the toast stack, but not when the stack is what is closing', () => {
    const home = mountHome()
    cleanup = home.unregister
    home.list.focus()
    const origin = captureFocusOrigin()
    const stack = focusable()
    stack.setAttribute('data-toast-stack', '')
    focusable(stack).focus()

    restoreFocus(origin)
    expect(home.refocus).not.toHaveBeenCalled()

    restoreFocus(origin, { leaving: stack })
    expect(home.refocus).toHaveBeenCalledTimes(1)
  })

  it('does nothing without an origin', () => {
    const home = mountHome()
    cleanup = home.unregister
    restoreFocus(null)
    expect(home.refocus).not.toHaveBeenCalled()
  })
})

describe('isFocusAdrift', () => {
  it('holds on <body> and on the home root, not on the list or anything else', () => {
    const home = mountHome()
    cleanup = home.unregister
    document.body.focus()
    expect(isFocusAdrift()).toBe(true)
    home.root.focus()
    expect(isFocusAdrift()).toBe(true)
    home.list.focus()
    expect(isFocusAdrift()).toBe(false)
    focusable(document.body, 'button').focus()
    expect(isFocusAdrift()).toBe(false)
  })

  it('never holds without a home (a window with no explorer)', () => {
    document.body.focus()
    expect(isFocusAdrift()).toBe(false)
  })
})

describe('focusHome', () => {
  it('refocuses the registered home, and answers false once it unregistered', () => {
    const home = mountHome()
    expect(focusHome()).toBe(true)
    expect(home.refocus).toHaveBeenCalledTimes(1)
    home.unregister()
    expect(focusHome()).toBe(false)
  })
})
