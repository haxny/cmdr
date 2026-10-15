import { describe, it, expect, afterEach } from 'vitest'
import { assertFocusModel, focusModelViolations } from './test-focus-model'

/**
 * One passing fixture, then one fixture per rule that breaks exactly that rule. Each
 * failing case asserts the violation's wording names the rule, so a check that silently
 * stopped running would fail here rather than pass everything.
 */

function mountHtml(html: string): HTMLElement {
  const host = document.createElement('div')
  host.innerHTML = html
  document.body.appendChild(host)
  return host
}

/** A focusable listbox naming its second option, and a polite region beside it. */
const GOOD_LIST = `
  <div class="root">
    <div role="listbox" tabindex="-1" aria-label="Files" aria-activedescendant="row-1">
      <div role="option" id="row-0" aria-selected="false">a.txt</div>
      <div role="option" id="row-1" aria-selected="false">b.txt</div>
    </div>
    <div class="announcer" aria-live="polite"></div>
  </div>`

function focusIn(host: HTMLElement, selector: string): void {
  host.querySelector<HTMLElement>(selector)?.focus()
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('focusModelViolations', () => {
  it('passes a focused listbox whose active descendant is unique, a named option, and inside it', () => {
    const host = mountHtml(GOOD_LIST)
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host.querySelector('.root'), { liveRegions: ['.announcer'] })).toEqual([])
    expect(() => {
      assertFocusModel(host.querySelector('.root'), { liveRegions: ['.announcer'] })
    }).not.toThrow()
  })

  it('passes a menu surface naming a menuitemcheckbox', () => {
    const host = mountHtml(`
      <div role="menu" tabindex="-1" aria-label="Volumes" aria-activedescendant="m-1">
        <div role="menuitemcheckbox" aria-checked="true" id="m-1">Fast connection</div>
      </div>`)
    focusIn(host, '[role="menu"]')
    expect(focusModelViolations(host.querySelector('[role="menu"]'))).toEqual([])
  })

  it('flags focus outside the root', () => {
    const host = mountHtml(GOOD_LIST)
    const violations = focusModelViolations(host.querySelector('.root'))
    expect(violations).toHaveLength(1)
    expect(violations[0]).toContain('focus is on <body>, outside')
  })

  it('flags a missing root', () => {
    expect(focusModelViolations(null)).toEqual(['root is missing'])
  })

  it('flags a focused list with rows but no active descendant', () => {
    const host = mountHtml(GOOD_LIST)
    host.querySelector('[role="listbox"]')?.removeAttribute('aria-activedescendant')
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host.querySelector('.root'))).toEqual([
      expect.stringContaining('has focus and rows, but no aria-activedescendant'),
    ])
  })

  it('lets an EMPTY focused list name no descendant', () => {
    const host = mountHtml(`<div role="listbox" tabindex="-1" aria-label="Files"></div>`)
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host)).toEqual([])
  })

  it('flags an id that two elements share (the two-pane row id collision)', () => {
    const host = mountHtml(GOOD_LIST + `<div role="option" id="row-1">b.txt in the other pane</div>`)
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host.querySelector('.root'))).toEqual([
      expect.stringContaining('"row-1" matches 2 elements'),
    ])
  })

  it('flags an id that resolves to nothing (a row virtualized away)', () => {
    const host = mountHtml(GOOD_LIST)
    host.querySelector('#row-1')?.remove()
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host.querySelector('.root'))).toEqual([
      expect.stringContaining('"row-1" matches 0 elements'),
    ])
  })

  it('flags a descendant outside the focused element (the sibling submenu shape)', () => {
    const host = mountHtml(`
      <div class="portal">
        <div role="menu" tabindex="-1" aria-label="Volumes" aria-activedescendant="sub-0"></div>
        <div role="menu" aria-label="Share"><div role="menuitem" id="sub-0">Eject</div></div>
      </div>`)
    focusIn(host, '[role="menu"]')
    expect(focusModelViolations(host.querySelector('.portal'))).toEqual([
      expect.stringContaining('neither inside nor owned by'),
    ])
  })

  it('accepts a descendant the focused element aria-owns', () => {
    const host = mountHtml(`
      <div class="portal">
        <div role="menu" tabindex="-1" aria-label="Volumes" aria-activedescendant="sub-0" aria-owns="sub"></div>
        <div role="menu" id="sub" aria-label="Share"><div role="menuitem" id="sub-0">Eject</div></div>
      </div>`)
    focusIn(host, '[role="menu"]')
    expect(focusModelViolations(host.querySelector('.portal'))).toEqual([])
  })

  it('flags a descendant with the wrong role', () => {
    const host = mountHtml(GOOD_LIST)
    host.querySelector('#row-1')?.setAttribute('role', 'button')
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host.querySelector('.root'))).toEqual([
      expect.stringContaining('not option or menuitem*'),
    ])
  })

  it('flags an unnamed cursor row, and skips that rule when asked', () => {
    const host = mountHtml(GOOD_LIST)
    const row = host.querySelector('#row-1')
    if (row) row.textContent = ''
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host.querySelector('.root'))).toEqual([
      expect.stringContaining('has no accessible name'),
    ])
    expect(focusModelViolations(host.querySelector('.root'), { requireCursorName: false })).toEqual([])
  })

  it('flags an expected live region that is missing, and one that is not live', () => {
    const host = mountHtml(GOOD_LIST)
    focusIn(host, '[role="listbox"]')
    expect(focusModelViolations(host.querySelector('.root'), { liveRegions: ['.missing'] })).toEqual([
      expect.stringContaining('.missing matches 0 elements'),
    ])
    host.querySelector('.announcer')?.removeAttribute('aria-live')
    expect(focusModelViolations(host.querySelector('.root'), { liveRegions: ['.announcer'] })).toEqual([
      expect.stringContaining('is not live'),
    ])
  })

  describe('focusMayBeContainer (the explorer container, when the focused pane shows no list)', () => {
    const CONTAINER = `<div class="explorer" tabindex="0" aria-label="File explorer">${GOOD_LIST}</div>`

    it('accepts focus on the root itself with no active descendant', () => {
      const host = mountHtml(CONTAINER)
      focusIn(host, '.explorer')
      expect(focusModelViolations(host.querySelector('.explorer'), { focusMayBeContainer: true })).toEqual([])
    })

    it('still requires the strict model without it', () => {
      const host = mountHtml(`<div class="explorer" tabindex="0" aria-label="File explorer">
        <div role="option" id="x">a.txt</div></div>`)
      focusIn(host, '.explorer')
      expect(focusModelViolations(host.querySelector('.explorer'))).toEqual([
        expect.stringContaining('no aria-activedescendant'),
      ])
    })

    it('still flags focus that left the root', () => {
      const host = mountHtml(CONTAINER)
      expect(focusModelViolations(host.querySelector('.explorer'), { focusMayBeContainer: true })).toEqual([
        expect.stringContaining('outside'),
      ])
    })
  })

  it('assertFocusModel throws one error listing every violation', () => {
    const host = mountHtml(GOOD_LIST)
    expect(() => {
      assertFocusModel(host.querySelector('.root'), { liveRegions: ['.missing'] })
    }).toThrow(/Focus model violated:\n- focus is on <body>.*\n- live region \.missing/s)
  })
})
