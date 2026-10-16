// "What's new" popup: fetch the changelog slice and the security advisories fixed since the last seen version, and
// read the dev simulate-update override.

import { commands } from '#lib/ipc/bindings.js'
import type { SecurityAdvisory, WhatsNewRelease, WhatsNewSection } from '#lib/ipc/bindings.js'

export type { SecurityAdvisory, WhatsNewRelease, WhatsNewSection }

/**
 * Returns the displayable releases in `sinceVersion < release <= current`, newest first,
 * truncated to `max`. `sinceVersion = null` means no lower bound (the latest `max`).
 *
 * The result can be empty even on a real "show" decision (every in-range release dropped
 * to nothing backend-side), so the auto-popup caller collapses an empty result to a stamp.
 */
export function getWhatsNew(sinceVersion: string | null, max: number): Promise<WhatsNewRelease[]> {
  return commands.getWhatsNew(sinceVersion, max)
}

/**
 * Reads the `CMDR_SIMULATE_UPDATE_FROM` dev flag (a backend-process env var invisible to
 * the Vite frontend). Returns the simulated "updated from" version, or `null` when unset.
 */
export function whatsNewDevOverride(): Promise<string | null> {
  return commands.whatsNewDevOverride()
}

/**
 * Returns the security advisories this build's own list says were fixed in `sinceVersion < fixedIn <= current`, most
 * severe first. Each link is already vetted against the advisory allowlist backend-side (`null` when refused).
 */
export function getSecurityAdvisories(sinceVersion: string): Promise<SecurityAdvisory[]> {
  return commands.getSecurityAdvisories(sinceVersion)
}
