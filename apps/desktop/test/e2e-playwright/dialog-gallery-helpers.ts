/**
 * Opening and closing dialog-gallery states from a spec, the way Debug > Soft dialogs
 * does. The gallery (`DIALOG_GALLERY_ENTRIES`) reaches every dialog in a known state with
 * no staging, so a spec that measures dialogs drives it through these.
 *
 * ❗ It needs a gallery-carrying binary: `CMDR_E2E_BUILD=1` (set by
 * `test:e2e:playwright:build` and the Linux Docker build) turns on the
 * `__CMDR_E2E_BUILD__` define. `skipWithoutGallery` turns its absence into a local skip
 * with the recipe, and a CI failure, since a silent skip there would read as coverage
 * that isn't happening.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { dismissOverlay, acceptOnboardingTermsIfPresent } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** The landmark set the dev-only fixture-directory command returns. */
export interface FixtureDirPayload {
  root: string
  destinationDir: string
  existingFolderName: string
  existingFileName: string
  nestedPath: string
}

/**
 * The disk-backed rows' throwaway directory, or `null` when this binary has no
 * `create_dialog_gallery_fixtures` (it's `#[cfg(debug_assertions)]`, which a plain
 * release build leaves off). Those rows then go unmeasured and say so, which beats
 * taking a whole sweep down with them.
 */
export async function createFixtureDir(page: TauriPage): Promise<FixtureDirPayload | null> {
  return page
    .evaluate<FixtureDirPayload>(`window.__TAURI_INTERNALS__.invoke('create_dialog_gallery_fixtures', {})`)
    .catch(() => null)
}

/**
 * Asks the main window to open one gallery state: emit `debug-open-gallery-dialog` and let
 * its own listener resolve fixtures, seed stores, or dispatch the app command, depending
 * on how that dialog is built (`openedBy` in the registry).
 */
export async function openGalleryState(
  page: TauriPage,
  dialogId: string,
  stateId: string,
  fixtures: FixtureDirPayload | null,
): Promise<void> {
  const payload = JSON.stringify({ dialogId, stateId, fixtures })
  await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
        event: 'debug-open-gallery-dialog',
        payload: ${payload}
    })`)
}

/**
 * Does this binary carry the gallery? Opens the one row that needs nothing staged
 * (the short alert) and looks for it. A direct capability test: probing the Rust
 * fixture command instead would answer a different question (that gate is
 * `debug_assertions`, the gallery's is a Vite define).
 */
export async function probeGallery(page: TauriPage): Promise<boolean> {
  await openGalleryState(page, 'alert', 'short', null)
  const live = await page
    .waitForSelector('[data-dialog-id="alert"]', 4000)
    .then(() => true)
    .catch(() => false)
  if (live) await dismissOverlay(page).catch(() => {})
  return live
}

/**
 * Skips the running test when this binary carries no gallery, or fails it under CI. No
 * gallery means no listener to answer the trigger, and every dialog would read as
 * "didn't open": locally that's a build to redo, in CI it's the check quietly not running.
 */
export async function skipWithoutGallery(page: TauriPage): Promise<void> {
  const galleryLive = await probeGallery(page)
  const noGallery =
    'This binary carries no dialog gallery (`CMDR_E2E_BUILD=1` sets the ' +
    '`__CMDR_E2E_BUILD__` define). Rebuild with `pnpm test:e2e:playwright:build`.'
  if (!galleryLive && process.env.CI) throw new Error(noGallery)
  test.skip(!galleryLive, noGallery)
}

/**
 * Closes whatever the gallery just opened, and reports whether it actually went.
 *
 * Escape closes a `ModalDialog` that has an `onclose`, but some surfaces refuse it on
 * purpose: the onboarding wizard, and a question whose every way out is an answer
 * (`sources-gone`, no `onclose`, so no × and no Escape). A surface left up swallows the
 * keystrokes of every spec after it on the shard. That's not hypothetical: it killed four
 * `file-operations` rename tests three specs downstream, each reporting a missing
 * `.rename-input`. So the fallback clicks the primary (rightmost) footer button until it
 * unmounts: the wizard's own forward contract (`onboarding.spec.ts`), and for a gallery
 * `ModalDialog` an answer whose fixture callback only closes the preview. The caller fails
 * on anything still open rather than carrying it.
 */
export async function closeGallerySurface(page: TauriPage, dialogId: string): Promise<boolean> {
  const selector = `[data-dialog-id="${dialogId}"]`
  const isOpen = async (): Promise<boolean> => page.isVisible(selector).catch(() => false)

  await dismissOverlay(page).catch(() => {})
  // Four steps plus slack, matching the wizard's own bound.
  for (let i = 0; i < 6 && (await isOpen()); i++) {
    // The onboarding wizard's Beta step blocks its forward button until the terms are ticked.
    await acceptOnboardingTermsIfPresent(page)
    await page.evaluate(`(function() {
        var btns = document.querySelectorAll('${selector} .primary-slot button, ${selector} .modal-footer-actions button');
        if (btns.length > 0) btns[btns.length - 1].click();
    })()`)
    await expect
      .poll(isOpen, { timeout: waitBudget(1000) })
      .toBeFalsy()
      .catch(() => {})
  }
  return !(await isOpen())
}
