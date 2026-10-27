/** The ▶/■ button of a track row: named for its file, and `aria-pressed` while the track plays. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import PlayButton from './PlayButton.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountButton(over: { playing?: boolean; disabled?: boolean } = {}) {
  const onToggle = vi.fn()
  const mounted = await mountInto(PlayButton, {
    fileName: '01.mp3',
    playing: over.playing ?? false,
    disabled: over.disabled ?? false,
    onToggle,
  })
  cleanup = mounted.cleanup
  return { button: mounted.root.querySelector('button') as HTMLButtonElement, onToggle }
}

describe('PlayButton', () => {
  it('is named "Play {file}" and not pressed while the track is idle', async () => {
    const { button } = await mountButton()
    expect(button.getAttribute('aria-label')).toBe('Play 01.mp3')
    expect(button.getAttribute('aria-pressed')).toBe('false')
  })

  it('is named "Stop {file}" and pressed while the track plays', async () => {
    const { button } = await mountButton({ playing: true })
    expect(button.getAttribute('aria-label')).toBe('Stop 01.mp3')
    expect(button.getAttribute('aria-pressed')).toBe('true')
  })

  it('a click toggles', async () => {
    const { button, onToggle } = await mountButton()
    button.click()
    expect(onToggle).toHaveBeenCalledOnce()
  })

  it('disabled does not toggle', async () => {
    const { button, onToggle } = await mountButton({ disabled: true })
    button.click()
    expect(onToggle).not.toHaveBeenCalled()
  })
})
