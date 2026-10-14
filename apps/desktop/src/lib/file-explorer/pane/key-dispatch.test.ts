import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { FilePaneAPI } from './types'
import { createKeyDispatch, isTypingInInput, type KeyDispatchDeps } from './key-dispatch'

function makePaneRef(overrides: Record<string, unknown> = {}) {
  return {
    isLoading: vi.fn(() => false),
    handleCancelLoading: vi.fn(),
    isHeaderMenuOpen: vi.fn(() => false),
    isRenaming: vi.fn(() => false),
    isJumpActive: vi.fn(() => false),
    handleJumpKeystroke: vi.fn(),
    clearJumpState: vi.fn(),
    isQuickFilterMode: vi.fn(() => false),
    isQuickFilterActive: vi.fn(() => false),
    appendQuickFilter: vi.fn(),
    backspaceQuickFilter: vi.fn(),
    clearQuickFilter: vi.fn(),
    handleKeyDown: vi.fn(),
    handleKeyUp: vi.fn(),
    ...overrides,
  }
}

function setup(refs: Partial<Record<'left' | 'right', ReturnType<typeof makePaneRef>>>, container?: HTMLElement) {
  const deps: KeyDispatchDeps = {
    getPaneRef: (p) => refs[p] as unknown as FilePaneAPI | undefined,
    getFocusedPane: () => 'left',
    getContainerElement: () => container,
  }
  return createKeyDispatch(deps)
}

/**
 * A KeyboardEvent with spied preventDefault/stopPropagation. The spies are also
 * returned as plain `preventDefaultSpy`/`stopPropagationSpy` properties so
 * assertions read a function-valued spy (not the class method) and don't trip
 * `@typescript-eslint/unbound-method`.
 */
function keyEvent(key: string) {
  const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
  const preventDefaultSpy = vi.spyOn(e, 'preventDefault')
  const stopPropagationSpy = vi.spyOn(e, 'stopPropagation')
  return Object.assign(e, { preventDefaultSpy, stopPropagationSpy })
}

describe('createKeyDispatch', () => {
  beforeEach(() => vi.clearAllMocks())

  it('Escape while loading cancels the load and swallows the key (no forward)', () => {
    const left = makePaneRef({ isLoading: vi.fn(() => true) })
    const kd = setup({ left })
    const e = keyEvent('Escape')

    kd.handleKeyDown(e)

    expect(left.handleCancelLoading).toHaveBeenCalled()
    expect(e.preventDefaultSpy).toHaveBeenCalled()
    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  // The switcher's own keys never reach here (its `Menu` catches them on a document capture
  // listener). What this covers is everything the menu deliberately let through — the inline
  // favorite-rename input's keystrokes — which must still not move the pane cursor behind it.
  it('an open volume chooser swallows the key from the pane behind (panes stay inert)', () => {
    const left = makePaneRef({ isHeaderMenuOpen: vi.fn(() => true) })
    const kd = setup({ left })

    kd.handleKeyDown(keyEvent('ArrowDown'))

    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('swallows for a chooser open on the OTHER pane too (⌥F2 opens one over an unfocused pane)', () => {
    const left = makePaneRef()
    const right = makePaneRef({ isHeaderMenuOpen: vi.fn(() => true) })
    const kd = setup({ left, right })

    kd.handleKeyDown(keyEvent('ArrowDown'))

    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('a printable letter is captured into the type-to-jump buffer and not forwarded', () => {
    const left = makePaneRef()
    const kd = setup({ left })
    const e = keyEvent('a')

    kd.handleKeyDown(e)

    expect(left.handleJumpKeystroke).toHaveBeenCalledWith('a')
    expect(e.preventDefaultSpy).toHaveBeenCalled()
    expect(e.stopPropagationSpy).toHaveBeenCalled()
    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('once a jump is active, a printable non-letter (like "-") extends the buffer', () => {
    const left = makePaneRef({ isJumpActive: vi.fn(() => true) })
    const kd = setup({ left })

    kd.handleKeyDown(keyEvent('-'))

    expect(left.handleJumpKeystroke).toHaveBeenCalledWith('-')
    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('a reset key clears the jump buffer and still forwards to the pane', () => {
    const left = makePaneRef()
    const kd = setup({ left })
    const e = keyEvent('ArrowDown')

    kd.handleKeyDown(e)

    expect(left.clearJumpState).toHaveBeenCalled()
    expect(left.handleKeyDown).toHaveBeenCalledWith(e)
  })

  it('does not intercept jump keys while renaming (forwards instead)', () => {
    const left = makePaneRef({ isRenaming: vi.fn(() => true) })
    const kd = setup({ left })
    const e = keyEvent('a')

    kd.handleKeyDown(e)

    expect(left.handleJumpKeystroke).not.toHaveBeenCalled()
    expect(left.handleKeyDown).toHaveBeenCalledWith(e)
  })

  it('handleKeyUp forwards to the focused pane', () => {
    const left = makePaneRef()
    const kd = setup({ left })
    const e = keyEvent('Shift')

    kd.handleKeyUp(e)

    expect(left.handleKeyUp).toHaveBeenCalledWith(e)
  })

  describe('handleFocusGuard', () => {
    it('refocuses the container when focus escapes to a non-exempt element', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const kd = setup({}, container)
      const button = document.createElement('button')

      kd.handleFocusGuard({ target: button } as unknown as FocusEvent)

      expect(focusSpy).toHaveBeenCalled()
    })

    it('leaves focus alone on input elements', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const kd = setup({}, container)
      const input = document.createElement('input')

      kd.handleFocusGuard({ target: input } as unknown as FocusEvent)

      expect(focusSpy).not.toHaveBeenCalled()
    })

    it('leaves focus alone inside dialog content (the trapFocus ping-pong guard)', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const kd = setup({}, container)
      const dialog = document.createElement('div')
      dialog.setAttribute('role', 'dialog')
      const inner = document.createElement('button')
      dialog.appendChild(inner)

      kd.handleFocusGuard({ target: inner } as unknown as FocusEvent)

      expect(focusSpy).not.toHaveBeenCalled()
    })

    it('leaves focus alone when the container itself receives focus', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const kd = setup({}, container)

      kd.handleFocusGuard({ target: container } as unknown as FocusEvent)

      expect(focusSpy).not.toHaveBeenCalled()
    })
  })

  /**
   * Characterization pins for the screen-reader focus work (#437): focus and key routing
   * as they are today, with real focus moves and real dispatches against a DOM shaped like
   * the explorer's. Two groups change on purpose once pane chrome becomes reachable from
   * the keyboard (#437 items 7–8): the guard stops bouncing chrome controls, and keys from a
   * focused chrome control stop reaching the pane. Those cases say so; nothing else here
   * may change.
   */
  describe('focus and key routing in an explorer-shaped DOM', () => {
    /**
     * The container with two panes, each with chrome (a tab, a path segment, a sort
     * header) and a listbox of two rows, wired the way `DualPaneExplorer` binds them.
     */
    function mountExplorer(focusedPane: () => 'left' | 'right' = () => 'left') {
      const container = document.createElement('div')
      container.className = 'dual-pane-explorer'
      container.tabIndex = 0
      container.innerHTML = ['left', 'right']
        .map(
          (side) => `
          <div class="file-pane" data-side="${side}">
            <div role="tablist"><button class="tab" role="tab" aria-selected="true">Tab</button></div>
            <div class="header"><button type="button" class="path-segment">Users</button></div>
            <div class="header-row"><button type="button" class="sortable-header">Name</button></div>
            <div class="brief-list" role="listbox" tabindex="-1" aria-label="File list">
              <div class="file-entry" role="option" aria-selected="true">a.txt</div>
              <div class="file-entry" role="option" aria-selected="false">b.txt</div>
            </div>
          </div>`,
        )
        .join('')
      document.body.appendChild(container)
      const left = makePaneRef()
      const right = makePaneRef()
      const kd = createKeyDispatch({
        getPaneRef: (p) => ({ left, right })[p] as unknown as FilePaneAPI,
        getFocusedPane: focusedPane,
        getContainerElement: () => container,
      })
      container.addEventListener('focusin', kd.handleFocusGuard)
      container.addEventListener('keydown', kd.handleKeyDown)
      const q = (side: 'left' | 'right', selector: string): HTMLElement => {
        const el = container.querySelector<HTMLElement>(`[data-side="${side}"] ${selector}`)
        if (!el) throw new Error(`no ${selector} in the ${side} pane`)
        return el
      }
      return { container, left, right, q }
    }

    afterEach(() => {
      document.body.innerHTML = ''
    })

    // Where the bounce LANDS is deliberately not pinned (the container today; the focused
    // pane's list once focus moves onto the lists), only that focus leaves the control and
    // stays in the explorer. Pane chrome: changes on purpose once the guard leaves focus on
    // `[data-pane-chrome]`.
    it.each([
      ['a path segment button', '.path-segment'],
      ['a pane tab', '[role="tab"]'],
      ['a sort header button', '.sortable-header'],
    ])('bounces focus that lands on %s, keeping it in the explorer', (_name, selector) => {
      const { container, q } = mountExplorer()
      const control = q('left', selector)
      control.focus()
      expect(document.activeElement).not.toBe(control)
      expect(container.contains(document.activeElement)).toBe(true)
    })

    it.each([
      ['the container', null],
      ["the focused pane's listbox", '[role="listbox"]'],
      ["the focused pane's cursor row", '.file-entry'],
      // Pane chrome: changes on purpose once keys from a focused chrome control stay there.
      ['a path segment button', '.path-segment'],
      ['a sort header button', '.sortable-header'],
    ])('forwards ↓ dispatched on %s to the focused pane only', (_name, selector) => {
      const { container, left, right, q } = mountExplorer()
      const target = selector === null ? container : q('left', selector)
      target.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }))
      expect(left.handleKeyDown).toHaveBeenCalledTimes(1)
      expect(right.handleKeyDown).not.toHaveBeenCalled()
    })

    it("routes by the store's focused pane, not by which pane the event came from", () => {
      const { left, right, q } = mountExplorer(() => 'right')
      q('left', '[role="listbox"]').dispatchEvent(
        new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
      )
      expect(right.handleKeyDown).toHaveBeenCalledTimes(1)
      expect(left.handleKeyDown).not.toHaveBeenCalled()
    })

    // Pane chrome: changes on purpose; a letter typed on a focused chrome control must not jump.
    it("sends a letter typed on a chrome button into the focused pane's type-to-jump", () => {
      const { left, right, q } = mountExplorer()
      q('left', '.path-segment').dispatchEvent(
        new KeyboardEvent('keydown', { key: 'b', bubbles: true, cancelable: true }),
      )
      expect(left.handleJumpKeystroke).toHaveBeenCalledWith('b')
      expect(right.handleJumpKeystroke).not.toHaveBeenCalled()
    })
  })

  it('isTypingInInput detects text-entry controls', () => {
    const input = document.createElement('input')
    expect(isTypingInInput({ target: input } as unknown as KeyboardEvent)).toBe(true)
    const div = document.createElement('div')
    expect(isTypingInInput({ target: div } as unknown as KeyboardEvent)).toBe(false)
  })
})
