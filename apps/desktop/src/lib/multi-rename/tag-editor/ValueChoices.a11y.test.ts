/** Tier 3 a11y tests for `ValueChoices.svelte`: closed, and open with its listbox. */
import { afterEach, describe, it } from 'vitest'
import { tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import ValueChoices from './ValueChoices.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountChoices(): Promise<HTMLElement> {
  const mounted = await mountInto(ValueChoices, {
    field: 'Album',
    choices: [
      { values: ['Kniha 1'], files: 12 },
      { values: ['Kniha 1 CD2'], files: 8 },
    ],
    disabled: false,
    onPick: () => {},
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('ValueChoices a11y', () => {
  it('closed has no violations', async () => {
    await expectNoA11yViolations(await mountChoices())
  })

  it('open has no violations', async () => {
    const root = await mountChoices()
    root.querySelector('button')?.click()
    await tick()
    await expectNoA11yViolations(document.body)
  })
})
