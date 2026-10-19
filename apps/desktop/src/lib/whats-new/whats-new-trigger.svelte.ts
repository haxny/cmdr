/**
 * Reactive state + startup trigger for the "What's new" popup. The pure decision logic
 * lives in `whats-new.ts`; this module is the thin effectful layer that reads/writes
 * settings, fetches the changelog slice over IPC, and flips the dialog open.
 *
 * `$state` lives here (not in `whats-new.ts`) because reactive state needs a `.svelte.ts`
 * file. `+page.svelte` mounts `WhatsNewDialog` against `whatsNewState.open` and calls
 * `runWhatsNewStartupTrigger(...)` once onboarding resolves, then re-attempts on
 * onboarding close (mirroring the update-toast re-attempt in `updater.svelte.ts`).
 */

import { getVersion } from '@tauri-apps/api/app'
import { getSetting, setSetting } from '#lib/settings/index.js'
import {
  getSecurityAdvisories,
  getWhatsNew,
  whatsNewDevOverride,
  type SecurityAdvisory,
  type WhatsNewRelease,
} from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { decideWhatsNew } from './whats-new'

const log = getAppLogger('whatsNew')

/** The manual Help reopen and the dev-override show the latest five releases. */
const MANUAL_MAX = 5

interface WhatsNewState {
  open: boolean
  releases: WhatsNewRelease[]
  /**
   * Security advisories fixed since the version the user last saw, most severe first, from this build's own list. Shown
   * as a notice above the releases. Empty on the manual reopen, which isn't tied to an update.
   */
  advisories: SecurityAdvisory[]
  /** `true` only for the manual Help reopen and the dev override, where an empty slice shows the empty state. */
  allowEmpty: boolean
}

export const whatsNewState = $state<WhatsNewState>({
  open: false,
  releases: [],
  advisories: [],
  allowEmpty: false,
})

export function closeWhatsNew(): void {
  whatsNewState.open = false
}

function stamp(current: string): void {
  setSetting('whatsNew.lastSeenVersion', current)
}

function openWith(releases: WhatsNewRelease[], advisories: SecurityAdvisory[], allowEmpty: boolean): void {
  whatsNewState.releases = releases
  whatsNewState.advisories = advisories
  whatsNewState.allowEmpty = allowEmpty
  whatsNewState.open = true
}

/**
 * The manual-reopen seam: opens the dialog manually (Help > What's new / command palette),
 * showing the latest five releases with no lower bound. Never stamps `lastSeenVersion`, works
 * regardless of the `showOnUpdate` setting, and shows the empty state when nothing is
 * displayable. The `help.whatsNew` command handler calls this.
 */
export async function openWhatsNew(): Promise<void> {
  if (whatsNewState.open) return // Idempotent: a menu/palette double-fire opens it once.
  try {
    const releases = await getWhatsNew(null, MANUAL_MAX)
    openWith(releases, [], true)
  } catch (e) {
    log.warn("Couldn't load the changelog for the What's new popup: {error}", { error: e })
  }
}

/**
 * The advisories fixed since `since`, or none when the lookup throws: losing the notice beats losing the changelog,
 * and the restart toast may already have said it.
 */
async function advisoriesSince(since: string): Promise<SecurityAdvisory[]> {
  try {
    return await getSecurityAdvisories(since)
  } catch (e) {
    log.warn("Couldn't load the security advisories for the What's new popup: {error}", { error: e })
    return []
  }
}

interface StartupGates {
  /** `isOnboarded`, read by the caller from `loadSettings()` (it lives outside the settings registry). */
  onboarded: boolean
  onboardingShowing: boolean
  otherStartupModalOpen: boolean
}

/**
 * Runs the automatic post-update decision once on startup (and again on onboarding close).
 * Reads `whatsNew.lastSeenVersion` / `whatsNew.showOnUpdate` / `isOnboarded`, resolves the
 * current version, and acts on the pure `decideWhatsNew` result.
 *
 * Dev override (`CMDR_SIMULATE_UPDATE_FROM`): when set, bypasses `decideWhatsNew` entirely.
 * It diffs from the given version, force-opens the dialog regardless of the setting /
 * onboarding / modals, and does NOT stamp, so every relaunch keeps showing it.
 */
export async function runWhatsNewStartupTrigger(gates: StartupGates): Promise<void> {
  if (whatsNewState.open) return

  let current: string
  try {
    current = await getVersion()
  } catch (e) {
    log.warn("Couldn't read the app version; skipping the What's new check: {error}", { error: e })
    return
  }

  // Dev override short-circuit: force the show path without stamping.
  const override = await whatsNewDevOverride().catch(() => null)
  if (override != null) {
    log.info("CMDR_SIMULATE_UPDATE_FROM={from} active: forcing the What's new popup", { from: override })
    try {
      const releases = await getWhatsNew(override, MANUAL_MAX)
      openWith(releases, await advisoriesSince(override), true)
    } catch (e) {
      log.debug("Couldn't load the changelog for the dev override: {error}", { error: e })
    }
    return
  }

  // Asked before deciding: an advisory shows even with changelogs off, and keeps an empty slice from silent-stamping
  // the notice away. Bounded below by the stamp, so a fresh install (no stamp) asks nothing.
  const lastSeen = getSetting('whatsNew.lastSeenVersion')
  const advisories = lastSeen === '' ? [] : await advisoriesSince(lastSeen)

  const decision = decideWhatsNew({
    lastSeen,
    current,
    enabled: getSetting('whatsNew.showOnUpdate'),
    hasSecurityNotice: advisories.length > 0,
    onboarded: gates.onboarded,
    onboardingShowing: gates.onboardingShowing,
    otherStartupModalOpen: gates.otherStartupModalOpen,
  })

  switch (decision.action) {
    case 'none':
    case 'wait':
      // `wait` deliberately does NOT stamp: retry on the next launch / onboarding close.
      return
    case 'stamp':
      stamp(current)
      return
    case 'show': {
      let releases: WhatsNewRelease[] = []
      try {
        if (decision.changelog) releases = await getWhatsNew(decision.since, decision.max)
      } catch (e) {
        // Don't stamp on a fetch failure: a later launch can still show the diff.
        log.warn("Couldn't load the changelog for the What's new popup: {error}", { error: e })
        return
      }
      // An empty slice on an auto-show collapses to a silent stamp (never an empty auto-popup), unless the security
      // notice has something to say.
      if (releases.length === 0 && advisories.length === 0) {
        stamp(current)
        return
      }
      openWith(releases, advisories, false)
      stamp(current)
      return
    }
  }
}
