/**
 * Dev-only switch for the GitHub issue #440 visual cue proposals (Debug > Visual cues).
 *
 * Each pick lands on `<html>` as a `data-cue-*` attribute that `app-visual-cues.css` keys its
 * option on; "off" removes it, so the default look is untouched until David picks. The Debug
 * window applies a pick to itself and sends it to the main window over `VISUAL_CUES_EVENT`
 * (`routes/(main)/listener-setup.ts`), where the file panes live. Nothing persists: a reload
 * goes back to the default look.
 */

import { simulateDisplayAccessibility } from '#lib/display-accessibility.js'

export type PaneCue = 'off' | 'header-bar' | 'cursor-ring' | 'hollow-unfocused'
export type MarkedCue = 'off' | 'bold' | 'edge-bar' | 'badge'
export type RingCue = 'off' | 'ink-outside' | 'ink-inside' | 'strong-accent'

export interface VisualCueProposals {
  pane: PaneCue
  marked: MarkedCue
  ring: RingCue
  /** Forces `html.increase-contrast` on without touching System Settings. */
  simulateIncreaseContrast: boolean
  /** Forces `html.differentiate-without-color` on without touching System Settings. */
  simulateDifferentiateWithoutColor: boolean
}

export const DEFAULT_VISUAL_CUE_PROPOSALS: VisualCueProposals = {
  pane: 'off',
  marked: 'off',
  ring: 'off',
  simulateIncreaseContrast: false,
  simulateDifferentiateWithoutColor: false,
}

/** The Debug → main window event carrying a `VisualCueProposals`. */
export const VISUAL_CUES_EVENT = 'debug-visual-cues'

function setCue(attribute: 'data-cue-pane' | 'data-cue-marked' | 'data-cue-ring', value: string): void {
  if (value === 'off') document.documentElement.removeAttribute(attribute)
  else document.documentElement.setAttribute(attribute, value)
}

let current: VisualCueProposals = { ...DEFAULT_VISUAL_CUE_PROPOSALS }

/** The picks last applied in this window, so the Debug panel survives a section switch. */
export function currentVisualCueProposals(): VisualCueProposals {
  return { ...current }
}

/** Applies a set of picks to this window's `<html>`. */
export function applyVisualCueProposals(proposals: VisualCueProposals): void {
  current = { ...proposals }
  setCue('data-cue-pane', proposals.pane)
  setCue('data-cue-marked', proposals.marked)
  setCue('data-cue-ring', proposals.ring)
  simulateDisplayAccessibility({
    increaseContrast: proposals.simulateIncreaseContrast,
    differentiateWithoutColor: proposals.simulateDifferentiateWithoutColor,
  })
}
