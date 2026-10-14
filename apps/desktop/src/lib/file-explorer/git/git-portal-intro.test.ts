/**
 * The once-ever "Cmdr has some git-related magic for you!" toast: when it fires,
 * and that it spends its flag the moment it does.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'

const { settings, toast, mode } = vi.hoisted<{
  settings: { values: Record<string, unknown>; getSetting: Mock; setSetting: Mock }
  toast: { addToast: Mock }
  mode: { isE2eRun: Mock }
}>(() => {
  const values: Record<string, unknown> = {}
  return {
    settings: {
      values,
      getSetting: vi.fn((id: string) => values[id]),
      setSetting: vi.fn((id: string, v: unknown) => {
        values[id] = v
      }),
    },
    toast: { addToast: vi.fn() },
    mode: { isE2eRun: vi.fn(() => false) },
  }
})

vi.mock('#lib/settings/index.js', () => ({ getSetting: settings.getSetting, setSetting: settings.setSetting }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: toast.addToast }))
vi.mock('#lib/app-mode.js', () => ({ isE2eRun: mode.isE2eRun }))
vi.mock('./GitPortalIntroToastContent.svelte', () => ({ default: 'GitPortalIntroToastContent' }))

import { maybeShowGitPortalIntro, GIT_PORTAL_INTRO_SEEN_KEY } from './git-portal-intro'

describe('maybeShowGitPortalIntro', () => {
  const goToGitDir = vi.fn()

  beforeEach(() => {
    vi.clearAllMocks()
    mode.isE2eRun.mockReturnValue(false)
    settings.values[GIT_PORTAL_INTRO_SEEN_KEY] = false
    settings.values['fileExplorer.git.showVirtualGitPortal'] = true
  })

  it('shows a persistent toast the first time a pane lands on a repo root, and spends the flag', () => {
    expect(maybeShowGitPortalIntro({ panePath: '/code/cmdr', repoRoot: '/code/cmdr', goToGitDir })).toBe(true)
    expect(toast.addToast).toHaveBeenCalledWith(
      'GitPortalIntroToastContent',
      expect.objectContaining({ dismissal: 'persistent', props: { goToGitDir } }),
    )
    expect(settings.values[GIT_PORTAL_INTRO_SEEN_KEY]).toBe(true)
  })

  it('shows it only once, even when both panes land on a repo root together', () => {
    maybeShowGitPortalIntro({ panePath: '/code/cmdr', repoRoot: '/code/cmdr', goToGitDir })
    expect(maybeShowGitPortalIntro({ panePath: '/code/other', repoRoot: '/code/other', goToGitDir })).toBe(false)
    expect(toast.addToast).toHaveBeenCalledTimes(1)
  })

  it('stays quiet in a subfolder of the repo, keeping the flag for the root', () => {
    expect(maybeShowGitPortalIntro({ panePath: '/code/cmdr/src', repoRoot: '/code/cmdr', goToGitDir })).toBe(false)
    expect(settings.values[GIT_PORTAL_INTRO_SEEN_KEY]).toBe(false)
  })

  it('stays quiet when the .git portal is switched off', () => {
    settings.values['fileExplorer.git.showVirtualGitPortal'] = false
    expect(maybeShowGitPortalIntro({ panePath: '/code/cmdr', repoRoot: '/code/cmdr', goToGitDir })).toBe(false)
  })

  it('stays quiet in an automated run, where a persistent toast would trip unrelated specs', () => {
    mode.isE2eRun.mockReturnValue(true)
    expect(maybeShowGitPortalIntro({ panePath: '/code/cmdr', repoRoot: '/code/cmdr', goToGitDir })).toBe(false)
  })

  it('stays quiet before the pane path is known', () => {
    expect(maybeShowGitPortalIntro({ panePath: null, repoRoot: '/code/cmdr', goToGitDir })).toBe(false)
  })
})
