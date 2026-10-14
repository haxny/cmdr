/**
 * The settings a restricted window (the viewer) may persist: the frontend's ONE copy of the write
 * allowlist, keyed by the backend's `RestrictedWindowPersistableSetting` enum. That enum is the
 * enforced boundary; the `satisfies` below makes a new variant a compile error until it's mapped
 * here, and `restricted-persistable-settings.test.ts` pins each id to Rust's `setting_id`.
 *
 * ❌ Shorter than the read snapshot ON PURPOSE. A setting earns a place here only when a restricted
 * window has a control that WRITES it (`W`, the binary-warning banner's button). One it merely
 * reads (`viewer.showTextCursor`, every `appearance.*`) stays out, so the app's highest-risk
 * webview never gets a write it has no use for.
 */

import type { RestrictedWindowPersistableSetting } from '#lib/ipc/bindings.js'
import type { SettingId } from './types'

export const RESTRICTED_PERSISTABLE_SETTINGS = {
  viewerWordWrap: 'viewer.wordWrap',
  fileViewerSuppressBinaryWarning: 'fileViewer.suppressBinaryWarning',
} as const satisfies Record<RestrictedWindowPersistableSetting, SettingId>

const variantById = new Map<string, RestrictedWindowPersistableSetting>(
  (Object.keys(RESTRICTED_PERSISTABLE_SETTINGS) as RestrictedWindowPersistableSetting[]).map((variant) => [
    RESTRICTED_PERSISTABLE_SETTINGS[variant],
    variant,
  ]),
)

/** The persist command's variant for `id`, or `undefined` when a restricted window may not persist it. */
export function restrictedPersistableVariant(id: string): RestrictedWindowPersistableSetting | undefined {
  return variantById.get(id)
}
