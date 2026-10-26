/**
 * The box must lay out the same checked and unchecked, or it hops when ticked.
 *
 * Ark puts `hidden` on the indicator while the box is unchecked, and the UA's `[hidden]` rule
 * would take it out of layout. The root is inline-flex, so the line takes its baseline from the
 * indicator: with it gone, the browser synthesizes one from the box's border, and the control
 * (plus the prose beside it) sat ~2px off between the two states.
 */

import { readFileSync } from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import Checkbox from './Checkbox.svelte'

describe('Checkbox layout across states', () => {
  it('Ark still marks the unchecked indicator `hidden` (the premise the CSS answers)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Checkbox, { target, props: { checked: false, ariaLabel: 'Pick me' } })
    await tick()
    expect(target.querySelector('.checkbox-indicator')?.hasAttribute('hidden')).toBe(true)
  })

  // Read at the source: component styles aren't applied under happy-dom, so a computed-style
  // assertion would pass on an empty string and prove nothing.
  it('keeps the indicator in layout despite `hidden`', () => {
    const source = readFileSync(path.join(import.meta.dirname, 'Checkbox.svelte'), 'utf8')
    const rule = /:global\(\.checkbox-indicator\) \{([^}]*)\}/.exec(source)?.[1] ?? ''
    expect(rule).toMatch(/display:\s*block;/)
  })
})
