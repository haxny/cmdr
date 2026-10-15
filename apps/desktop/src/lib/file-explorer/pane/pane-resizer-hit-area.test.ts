/**
 * The pane divider's invisible grab strip (`PaneResizer.svelte`, `.pane-resizer::before`). Its geometry lives in CSS
 * and the test DOM does no layout, so this pins the declared offsets: wide enough to hit (#441, WCAG 2.5.8), and
 * never over the LEFT pane, whose vertical scrollbar runs along the divider and must stay fully grabbable.
 */
import { describe, expect, it } from 'vitest'
import source from './PaneResizer.svelte?raw'

/** The `left` / `right` offsets in px of the `.pane-resizer::before` rule. */
function grabStripOffsets(): { left: number; right: number } {
  const rule = /\.pane-resizer::before\s*\{([^}]*)\}/.exec(source)?.[1]
  if (rule === undefined) throw new Error('`.pane-resizer::before` rule not found')
  const offset = (side: 'left' | 'right'): number => {
    const value = new RegExp(`(?:^|;|\\s)${side}:\\s*(-?[\\d.]+)(px)?\\s*;`).exec(rule)?.[1]
    if (value === undefined) throw new Error(`\`${side}\` missing from the grab strip`)
    return Number(value)
  }
  return { left: offset('left'), right: offset('right') }
}

describe('pane divider grab strip', () => {
  it('never reaches over the left pane, so its scrollbar stays fully grabbable', () => {
    expect(grabStripOffsets().left).toBeGreaterThanOrEqual(0)
  })

  it('is at least 8px wide, counting the 1px divider', () => {
    const { left, right } = grabStripOffsets()
    expect(1 - left - right).toBeGreaterThanOrEqual(8)
  })
})
