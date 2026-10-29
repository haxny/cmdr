/**
 * Tier 3 a11y + behavior tests for `AlreadyGoneToastContent.svelte`: the note
 * after a delete or trash found its items already gone, with one button that
 * turns the note off for good (back on in Settings > Advanced).
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

import AlreadyGoneToastContent from './AlreadyGoneToastContent.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import { dismissToast } from '#lib/ui/toast/index.js'
import { setSetting } from '#lib/settings/index.js'

vi.mock('#lib/ui/toast/index.js', () => ({
  dismissToast: vi.fn(),
}))
vi.mock('#lib/settings/index.js', () => ({
  setSetting: vi.fn(),
}))

const message = '“song.m4a” was already gone. Another app probably moved or deleted it.'

function mountToast(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AlreadyGoneToastContent, { target, props: { toastId: 'already-gone', message } })
  return target
}

describe('AlreadyGoneToastContent', () => {
  beforeEach(() => {
    vi.mocked(dismissToast).mockClear()
    vi.mocked(setSetting).mockClear()
  })

  it('default render has no a11y violations', async () => {
    const target = mountToast()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('shows the message and the opt-out button', () => {
    const target = mountToast()
    expect(target.textContent).toContain(message)
    expect(target.textContent).toContain('Never show this again')
  })

  it('Never show this again turns the note off and closes the toast', async () => {
    const target = mountToast()
    await tick()
    const button = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Never show this again',
    )
    if (!button) throw new Error('Never show this again button missing')
    button.click()
    expect(setSetting).toHaveBeenCalledWith('fileOperations.showAlreadyGoneToast', false)
    expect(dismissToast).toHaveBeenCalledWith('already-gone')
  })
})
