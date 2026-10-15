import { describe, it, expect, vi, beforeEach } from 'vitest'
import { applyVisualCueProposals, DEFAULT_VISUAL_CUE_PROPOSALS } from './visual-cue-proposals'

const simulate = vi.hoisted(() => vi.fn())
vi.mock('#lib/display-accessibility.js', () => ({ simulateDisplayAccessibility: simulate }))

const root = document.documentElement

describe('visual cue proposals', () => {
  beforeEach(() => {
    simulate.mockClear()
    for (const name of ['data-cue-pane', 'data-cue-marked', 'data-cue-ring']) root.removeAttribute(name)
  })

  it('puts each picked option on <html> as its data-cue attribute', () => {
    applyVisualCueProposals({
      ...DEFAULT_VISUAL_CUE_PROPOSALS,
      pane: 'header-bar',
      marked: 'edge-bar',
      ring: 'ink-inside',
    })
    expect(root.dataset.cuePane).toBe('header-bar')
    expect(root.dataset.cueMarked).toBe('edge-bar')
    expect(root.dataset.cueRing).toBe('ink-inside')
  })

  it('removes the attribute for "off", so the default look comes back', () => {
    applyVisualCueProposals({ ...DEFAULT_VISUAL_CUE_PROPOSALS, pane: 'cursor-ring' })
    applyVisualCueProposals(DEFAULT_VISUAL_CUE_PROPOSALS)
    expect(root.hasAttribute('data-cue-pane')).toBe(false)
    expect(root.hasAttribute('data-cue-marked')).toBe(false)
    expect(root.hasAttribute('data-cue-ring')).toBe(false)
  })

  it('forwards the display option simulations', () => {
    applyVisualCueProposals({
      ...DEFAULT_VISUAL_CUE_PROPOSALS,
      simulateIncreaseContrast: true,
      simulateDifferentiateWithoutColor: false,
    })
    expect(simulate).toHaveBeenCalledWith({ increaseContrast: true, differentiateWithoutColor: false })
  })
})
