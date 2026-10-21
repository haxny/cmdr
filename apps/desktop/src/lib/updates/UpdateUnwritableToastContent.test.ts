/**
 * Copy and a11y tests for `UpdateUnwritableToastContent.svelte`: the notice for an update this user can't write into
 * the bundle. What it SAYS is the feature, and it says two different things: an administrator can replace the app
 * themselves, anyone else has to ask.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import UpdateUnwritableToastContent from './UpdateUnwritableToastContent.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

const { openExternalUrlMock } = vi.hoisted(() => ({ openExternalUrlMock: vi.fn(() => Promise.resolve()) }))

vi.mock('#lib/tauri-commands/index.js', () => ({ openExternalUrl: openExternalUrlMock }))

async function render(blocker: 'notWritableAdminUser' | 'notWritableStandardUser'): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(UpdateUnwritableToastContent, {
    target,
    props: { version: '0.54.0', blocker, bundlePath: '/Applications/Cmdr.app' },
  })
  await tick()
  return target
}

describe('UpdateUnwritableToastContent', () => {
  it('tells an administrator to download the app again and drag it in, naming the real path', async () => {
    const target = await render('notWritableAdminUser')
    expect(target.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'Cmdr 0.54.0 is ready, but Cmdr can’t install it itself ' +
        '/Applications/Cmdr.app belongs to another account. To update, download Cmdr from getcmdr.com and drag it into Applications. macOS will ask for your password.',
    )
  })

  it('opens getcmdr.com from the link', async () => {
    const target = await render('notWritableAdminUser')
    const link = [...target.querySelectorAll('a, button')].find((el) => el.textContent?.trim() === 'getcmdr.com')
    expect(link).toBeDefined()
    ;(link as HTMLElement).click()
    expect(openExternalUrlMock).toHaveBeenCalledWith('https://getcmdr.com')
  })

  it('tells a standard user to ask whoever manages the Mac', async () => {
    const target = await render('notWritableStandardUser')
    expect(target.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'Cmdr 0.54.0 is out ' +
        'Your account can’t update Cmdr on this Mac. Ask your IT team, or whoever manages this Mac, to update it.',
    )
  })

  it.each(['notWritableAdminUser', 'notWritableStandardUser'] as const)(
    'has no a11y violations for %s',
    async (blocker) => {
      await expectNoA11yViolations(await render(blocker))
    },
  )
})
