import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { initGlassMaterial, cleanupGlassMaterial, DEFAULT_GLASS_TINT } from './glass-material'

// `vi.mock` is hoisted above the imports, so its factory can only reach these through `vi.hoisted`.
interface MockState {
  listeners: { tint?: (p: { amount: number | null }) => void }
  tintAmount: number | null
}
const state = vi.hoisted((): MockState => ({ listeners: {}, tintAmount: 0.27 }))

vi.mock('#lib/tauri-commands/index.js', () => ({
  getGlassTintAmount: () => Promise.resolve(state.tintAmount),
  onGlassTintChanged: (handler: (p: { amount: number | null }) => void) => {
    state.listeners.tint = handler
    return Promise.resolve(() => {})
  },
}))

const root = document.documentElement
const tintVar = () => root.style.getPropertyValue('--glass-tint')

describe('glass material', () => {
  beforeEach(() => {
    state.tintAmount = 0.27
    root.removeAttribute('style')
  })

  afterEach(() => {
    cleanupGlassMaterial()
  })

  it('exposes the Liquid Glass slider as --glass-tint', async () => {
    await initGlassMaterial()
    expect(tintVar()).toBe('0.27')
  })

  it('falls back to the middle of the slider when macOS reports none', async () => {
    state.tintAmount = null
    await initGlassMaterial()
    expect(tintVar()).toBe(String(DEFAULT_GLASS_TINT))
  })

  it('follows a slider move live', async () => {
    await initGlassMaterial()
    state.listeners.tint?.({ amount: 0.9 })
    expect(tintVar()).toBe('0.9')
    state.listeners.tint?.({ amount: null })
    expect(tintVar()).toBe(String(DEFAULT_GLASS_TINT))
  })
})
