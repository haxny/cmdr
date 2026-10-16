/**
 * Pure trigger logic for the "What's new" popup. No `$state`, no IPC: this module is
 * deliberately pure so the decision truth table is unit-testable in isolation. The
 * reactive startup effect that calls it lives in `whats-new-trigger.svelte.ts`.
 *
 * The full intention behind each branch (the "when the popup shows" rules) is in
 * the colocated `CLAUDE.md`.
 */

import { compareVersions } from '#lib/utils/version.js'

/** What the startup effect should do with this launch. */
export type WhatsNewDecision =
  /**
   * Open the dialog with this slice, then stamp `lastSeen = current`. `changelog: false` shows only the security notice:
   * the person turned changelogs off, which isn't turning security advisories off.
   */
  | { action: 'show'; since: string | null; max: number; changelog: boolean }
  /** Write `lastSeen = current`, no dialog (fresh install, downgrade, or feature off). */
  | { action: 'stamp' }
  /** A would-show is blocked by onboarding or another startup modal. Don't stamp; retry later. */
  | { action: 'wait' }
  /** Version unchanged: nothing to do. */
  | { action: 'none' }

export interface DecideWhatsNewArgs {
  /** The version we last showed the user, or `''` when never stamped. */
  lastSeen: string
  /** The version running right now (`getVersion()`). */
  current: string
  /** `whatsNew.showOnUpdate`: governs only the automatic popup's changelog. */
  enabled: boolean
  /**
   * A security advisory was fixed in `lastSeen < fixedIn <= current` (this build's own list). It shows even with
   * `enabled` off, and like any show it waits out onboarding rather than being stamped away.
   */
  hasSecurityNotice: boolean
  /** `isOnboarded`: the discriminator between a fresh install and an existing user. */
  onboarded: boolean
  /** The onboarding wizard (or legacy FDA modal) is on screen. */
  onboardingShowing: boolean
  /** Another startup modal (crash report, expiration) is on screen. */
  otherStartupModalOpen: boolean
}

/** A would-show is gated when onboarding or another startup modal is up. */
function isGated(args: DecideWhatsNewArgs): boolean {
  return args.onboardingShowing || args.otherStartupModalOpen
}

export function decideWhatsNew(args: DecideWhatsNewArgs): WhatsNewDecision {
  // No stored version means "never stamped". `isOnboarded` disambiguates the two cases:
  // a genuine fresh install (not onboarded) versus an existing user updating into the
  // feature (onboarded). Get this backwards and either every fresh install eats a
  // changelog popup, or the release that ships the feature never demonstrates it.
  if (args.lastSeen === '') {
    if (!args.onboarded) {
      // Fresh install: onboarding owns the first launch, not a changelog. Silent stamp.
      return { action: 'stamp' }
    }
    // Inaugural showcase: show off the feature with the current release only.
    if (!args.enabled) return { action: 'stamp' }
    if (isGated(args)) return { action: 'wait' }
    return { action: 'show', since: null, max: 1, changelog: true }
  }

  const order = compareVersions(args.current, args.lastSeen)

  // Version unchanged: nothing to do (don't even re-stamp).
  if (order === 0) return { action: 'none' }

  // Downgrade: rewrite `lastSeen` to current so a later re-upgrade behaves sanely.
  if (order < 0) return { action: 'stamp' }

  // Upgrade. Feature off → stamp silently (no backlog replay when re-enabled later), unless a security fix landed:
  // then the notice shows on its own.
  if (!args.enabled && !args.hasSecurityNotice) return { action: 'stamp' }
  if (isGated(args)) return { action: 'wait' }
  return { action: 'show', since: args.lastSeen, max: 5, changelog: args.enabled }
}
