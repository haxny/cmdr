/**
 * `MaskInput.svelte` under a foobar2000 pattern: its `[ ]` aren't Cmdr placeholders, so no ▾ marker and
 * no editor; and the character the engine says the mask breaks at is underlined.
 */
import { describe, it, expect, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountField(initial: string, props: { tokenEditors?: boolean; errorAt?: number | null }): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const harness = mount(MaskInputFixture, {
    target,
    props: { initial, onValue: () => {}, onOuterKey: () => {}, ...props },
  })
  cleanup = () => {
    void unmount(harness)
  }
  for (let i = 0; i < 4; i++) await tick()
}

describe('MaskInput in another syntax', () => {
  it('draws Cmdr token markers only with token editors on', async () => {
    await mountField('[C] [=tag.title]', {})
    expect(document.querySelectorAll('.token-marker').length).toBeGreaterThan(0)
    cleanup?.()
    document.body.innerHTML = ''
    await mountField('[C] [=tag.title]', { tokenEditors: false })
    expect(document.querySelectorAll('.token-marker')).toHaveLength(0)
  })

  it('underlines where the mask breaks, and nothing without an error', async () => {
    await mountField('Deník %nope%', { tokenEditors: false, errorAt: 6 })
    expect(document.querySelector('.error-mark')).not.toBeNull()
    cleanup?.()
    document.body.innerHTML = ''
    await mountField('Deník %title%', { tokenEditors: false })
    expect(document.querySelector('.error-mark')).toBeNull()
  })
})
