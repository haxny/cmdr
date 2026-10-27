/** Tier 3 a11y tests for `PlayButton.svelte`: idle, playing and disabled. */
import { afterEach, describe, it } from 'vitest'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import PlayButton from './PlayButton.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountButton(playing: boolean, disabled = false): Promise<HTMLElement> {
  const mounted = await mountInto(PlayButton, { fileName: '01.mp3', playing, disabled, onToggle: () => {} })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('PlayButton a11y', () => {
  it('idle has no violations', async () => {
    await expectNoA11yViolations(await mountButton(false))
  })

  it('playing has no violations', async () => {
    await expectNoA11yViolations(await mountButton(true))
  })

  it('disabled has no violations', async () => {
    await expectNoA11yViolations(await mountButton(false, true))
  })
})
