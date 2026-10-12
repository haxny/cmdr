/** Tier 3 a11y test for `KeyHint.svelte`: the key chip inside a button stays out of its name. */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import KeyHint from './KeyHint.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

describe('KeyHint a11y', () => {
  it('has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(KeyHint, { target, props: { key: 'S' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
