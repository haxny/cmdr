/** The run panel's words: a pause that waits for the step under way, a cancel that can't be pressed twice, and the folders a run leaves. */
import { afterEach, describe, expect, it } from 'vitest'
import { tick } from 'svelte'
import { report, settle } from './sync-test-fixtures'
import { running } from './sync-run-panel-fixture'

afterEach(() => {
  document.body.innerHTML = ''
})

const buttons = (target: HTMLElement) => [...target.querySelectorAll<HTMLButtonElement>('.actions button')]

describe('SyncRunPanel', () => {
  it('Pause says it waits for the step under way; Cancel says it is stopping and turns off', async () => {
    const { target } = await running()
    buttons(target)[1]?.click()
    await settle()
    await tick()
    expect(target.querySelector('.note')?.textContent.trim()).toBe('Pausing after this step…')
    buttons(target)[0]?.click()
    await settle()
    await tick()
    const [cancel, pause] = buttons(target)
    expect(cancel.textContent.trim()).toBe('Stopping…')
    expect(cancel.disabled).toBe(true)
    expect(pause.disabled).toBe(true)
  })

  it('the result lists the folders the run leaves, and that they stay', async () => {
    const { fake, target } = await running()
    fake.runFinished(
      report({
        emptyFolders: [{ side: 'right', path: '/b/new' }],
        leftoverFolders: [{ place: 'target', side: 'right', path: '/b/.cmdr-sync/versions/2026-10-31' }],
      }),
    )
    await settle()
    await tick()
    const text = target.querySelector('.left-behind')?.textContent ?? ''
    expect(text).toContain('These folders stay; the run never removes them:')
    expect(text).toContain('/b/new (empty: its copies didn’t land)')
    expect(text).toContain('/b/.cmdr-sync/versions/2026-10-31 (made for kept files, couldn’t be removed again)')
  })
})
