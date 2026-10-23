/**
 * Tests for the sort-shortcut tooltip on column headers.
 *
 * The tooltip must be truthful: the sort shortcut acts on the focused pane only,
 * so it appears only when the header's pane is focused, and it must track both
 * focus flips and shortcut rebinds live (the binding is user-customizable).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import SortableHeader from './SortableHeader.svelte'
import { setShortcut, resetShortcut } from '#lib/shortcuts/shortcuts-store.js'
import type { SortColumn } from '../types'

const TOOLTIP_SHOW_DELAY_MS = 400

interface HeaderProps {
  column: SortColumn
  label: string
  currentSortColumn: SortColumn
  currentSortOrder: 'ascending' | 'descending'
  onClick: (column: SortColumn) => void
  isFocused: boolean
  fallbackNote?: string
}

function setup(overrides: Partial<HeaderProps> = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const props: HeaderProps = $state({
    column: 'size',
    label: 'Size',
    currentSortColumn: 'name',
    currentSortOrder: 'ascending',
    onClick: vi.fn(),
    isFocused: true,
    ...overrides,
  })
  const component = mount(SortableHeader, { target, props })
  flushSync() // Run the effect phase so the `use:tooltip` action attaches its listeners
  const button = target.querySelector('button')
  if (!button) throw new Error('SortableHeader did not render a button')
  return { target, props, component, button }
}

function hover(button: HTMLElement): void {
  button.dispatchEvent(new MouseEvent('mouseenter'))
  vi.advanceTimersByTime(TOOLTIP_SHOW_DELAY_MS + 10)
}

function tooltipEl(): HTMLElement | null {
  return document.querySelector('.cmdr-tooltip')
}

function tooltipKbd(): HTMLElement | null {
  return tooltipEl()?.querySelector('kbd.cmdr-tooltip-kbd') ?? null
}

describe('SortableHeader sort-shortcut tooltip', () => {
  let cleanups: (() => void)[] = []

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    for (const cleanup of cleanups) cleanup()
    cleanups = []
    resetShortcut('sort.bySize')
    vi.useRealTimers()
  })

  function setupTracked(overrides: Partial<HeaderProps> = {}) {
    const result = setup(overrides)
    cleanups.push(() => {
      result.button.dispatchEvent(new MouseEvent('mouseleave'))
      void unmount(result.component)
      result.target.remove()
    })
    return result
  }

  it('shows the command name and the shortcut when the pane is focused', () => {
    const { button } = setupTracked()
    hover(button)

    expect(tooltipEl()?.textContent).toContain('Sort by size')
    // The default binding is ⌘6 (Ctrl+6 off-macOS); both contain '6'
    expect(tooltipKbd()?.textContent).toContain('6')
  })

  it('omits the shortcut when the pane is not focused (it would sort the other pane)', () => {
    const { button } = setupTracked({ isFocused: false })
    hover(button)

    expect(tooltipEl()?.textContent).toContain('Sort by size')
    expect(tooltipKbd()).toBeNull()
  })

  it('removes the shortcut live when focus flips away mid-hover', () => {
    const { button, props } = setupTracked()
    hover(button)
    expect(tooltipKbd()).not.toBeNull()

    props.isFocused = false
    flushSync()

    expect(tooltipEl()?.textContent).toContain('Sort by size')
    expect(tooltipKbd()).toBeNull()
  })

  it('updates the shown shortcut live when the user rebinds it', () => {
    const { button } = setupTracked()
    hover(button)
    expect(tooltipKbd()?.textContent).toContain('6')

    setShortcut('sort.bySize', 0, '⌘9')
    flushSync()

    expect(tooltipKbd()?.textContent).toBe('⌘9')
  })

  it('shows no shortcut for a column whose sort command has no binding', () => {
    const { button } = setupTracked({ column: 'created', label: 'Created' })
    hover(button)

    expect(tooltipEl()?.textContent).toContain('Sort by date created')
    expect(tooltipKbd()).toBeNull()
  })
})

// A requested date sort the location can't honor falls back to Modified; the header says so
// with a glyph, and the button's tooltip carries the sentence above its command name.
describe('SortableHeader fallback note', () => {
  const NOTE = 'This location doesn’t record when items were added.'

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('draws the info glyph inside the button, hidden from the a11y tree, with no tooltip of its own', () => {
    const header = setup({ column: 'added', label: 'Modified', fallbackNote: NOTE })
    const glyph = header.button.querySelector('.status-glyph')
    expect(glyph).not.toBeNull()
    expect(glyph?.closest('[aria-hidden="true"]')).not.toBeNull()

    glyph?.dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(TOOLTIP_SHOW_DELAY_MS + 10)
    expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()

    void unmount(header.component)
    header.target.remove()
  })

  it('puts the note above the command name in the button tooltip', () => {
    const header = setup({ column: 'added', label: 'Modified', fallbackNote: NOTE })
    hover(header.button)

    expect(tooltipEl()?.querySelector('.cmdr-tooltip-text')?.textContent).toBe(`${NOTE}\n\nSort by date added`)

    header.button.dispatchEvent(new MouseEvent('mouseleave'))
    void unmount(header.component)
    header.target.remove()
  })

  it('draws no glyph without a note', () => {
    const header = setup({ column: 'added', label: 'Added' })
    expect(header.button.querySelector('.status-glyph')).toBeNull()
    void unmount(header.component)
    header.target.remove()
  })
})

// `aria-sort` needs a `columnheader`, and these are plain buttons, so the sort state rides
// in the button's name. It starts with the visible label (WCAG 2.5.3, label in name).
describe('SortableHeader sort state, spoken', () => {
  it('names the active column with its direction, and every other column as not sorted', () => {
    const active = setup({ column: 'size', currentSortColumn: 'size', currentSortOrder: 'descending' })
    expect(active.button.getAttribute('aria-label')).toBe('Size, sorted descending')

    const ascending = setup({ column: 'size', currentSortColumn: 'size', currentSortOrder: 'ascending' })
    expect(ascending.button.getAttribute('aria-label')).toBe('Size, sorted ascending')

    const other = setup({ column: 'name', label: 'Name', currentSortColumn: 'size' })
    expect(other.button.getAttribute('aria-label')).toBe('Name, not sorted')

    for (const header of [active, ascending, other]) {
      void unmount(header.component)
      header.target.remove()
    }
  })

  // In a fallback the name carries the note too, after the state, so the glyph is never the
  // only place it lives (the glyph itself is `aria-hidden`).
  it('says why the rows follow another date, after the label and the state', () => {
    const note = 'This location doesn’t record when items were added, so Cmdr sorts by date modified here.'
    const header = setup({
      column: 'added',
      label: 'Added',
      currentSortColumn: 'added',
      currentSortOrder: 'descending',
      fallbackNote: note,
    })
    expect(header.button.getAttribute('aria-label')).toBe(`Added, sorted descending (${note})`)
    void unmount(header.component)
    header.target.remove()
  })

  it('follows a sort change live', () => {
    const header = setup({ column: 'size', currentSortColumn: 'name' })
    header.props.currentSortColumn = 'size'
    flushSync()
    expect(header.button.getAttribute('aria-label')).toBe('Size, sorted ascending')
    void unmount(header.component)
    header.target.remove()
  })
})
