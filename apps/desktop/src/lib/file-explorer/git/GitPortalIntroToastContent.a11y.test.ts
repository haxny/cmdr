/**
 * Tier 3 a11y + behavior test for `GitPortalIntroToastContent.svelte`: the
 * once-ever intro to the `.git` portal. Three buttons, the middle one only while
 * hidden files are hidden.
 */
import { describe, it, vi, expect, beforeEach, type Mock } from 'vitest'
import { mount, tick, flushSync } from 'svelte'

const { settings, toast } = vi.hoisted<{
  settings: { values: Record<string, unknown>; listeners: ((v: unknown) => void)[]; setSetting: Mock }
  toast: { dismissToast: Mock }
}>(() => {
  const values: Record<string, unknown> = {}
  const listeners: ((v: unknown) => void)[] = []
  return {
    settings: {
      values,
      listeners,
      setSetting: vi.fn((id: string, v: unknown) => {
        values[id] = v
        for (const l of listeners) l(v)
      }),
    },
    toast: { dismissToast: vi.fn() },
  }
})

vi.mock('#lib/settings/index.js', () => ({
  getSetting: (id: string) => settings.values[id],
  setSetting: settings.setSetting,
  onSpecificSettingChange: (_id: string, cb: (v: unknown) => void) => {
    settings.listeners.push(cb)
    return () => {}
  },
}))
vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast: toast.dismissToast }))

import GitPortalIntroToastContent from './GitPortalIntroToastContent.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

function mountToast(goToGitDir = vi.fn()) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(GitPortalIntroToastContent, { target, props: { toastId: 'git-portal-intro', goToGitDir } })
  return { target, goToGitDir }
}

const buttonNamed = (target: HTMLElement, name: string) =>
  Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === name)

describe('GitPortalIntroToastContent', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    settings.listeners.length = 0
    settings.values['listing.showHiddenFiles'] = false
  })

  it('default render has no a11y violations', async () => {
    const { target } = mountToast()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('says what the portal offers and where it lives', () => {
    const { target } = mountToast()
    expect(target.textContent).toContain('Cmdr has some git-related magic for you!')
    expect(target.textContent).toContain('.git')
  })

  it('Show hidden files flips the same setting the shortcut does, then hides itself', () => {
    const { target } = mountToast()
    const show = buttonNamed(target, 'Show hidden files')
    if (!show) throw new Error('Show hidden files button missing')
    show.click()
    flushSync()
    expect(settings.setSetting).toHaveBeenCalledWith('listing.showHiddenFiles', true)
    expect(buttonNamed(target, 'Show hidden files')).toBeUndefined()
  })

  it('leaves out Show hidden files when hidden files already show', () => {
    settings.values['listing.showHiddenFiles'] = true
    const { target } = mountToast()
    expect(buttonNamed(target, 'Show hidden files')).toBeUndefined()
  })

  it('Go to .git takes the pane there and closes the toast', () => {
    const { target, goToGitDir } = mountToast()
    buttonNamed(target, 'Go to .git')?.click()
    expect(goToGitDir).toHaveBeenCalled()
    expect(toast.dismissToast).toHaveBeenCalledWith('git-portal-intro')
  })

  it('Dismiss closes the toast', () => {
    const { target } = mountToast()
    buttonNamed(target, 'Dismiss')?.click()
    expect(toast.dismissToast).toHaveBeenCalledWith('git-portal-intro')
  })
})
