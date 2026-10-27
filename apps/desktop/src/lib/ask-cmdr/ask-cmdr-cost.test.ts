/**
 * Unit tests for the pure cost-formatting helpers (the per-thread footer + settings spend).
 * `formatUsdMicros` reads the locale via the intl layer; these assert en-US output.
 */

import { describe, it, expect, beforeAll } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import type { ConversationCost } from '#lib/tauri-commands/index.js'
import { costReading, formatUsdMicros, isLocalOnly, totalTokens } from './ask-cmdr-cost'

beforeAll(() => {
  _setLocaleForTests('en-US')
})

describe('totalTokens', () => {
  it('sums prompt and completion tokens', () => {
    expect(totalTokens({ promptTokens: 300, completionTokens: 70 })).toBe(370)
  })
})

describe('formatUsdMicros', () => {
  it('shows up to four fraction digits for sub-dollar amounts (never rounds a real cost to $0.00)', () => {
    // 1,200 micro-USD = $0.0012 (four digits, exact).
    expect(formatUsdMicros(1_200)).toBe('$0.0012')
    // A tiny real cost still shows a non-zero amount, never a misleading $0.00.
    expect(formatUsdMicros(400)).toBe('$0.0004')
  })

  it('rounds to cents for a dollar or more', () => {
    // 1,230,000 micro-USD = $1.23.
    expect(formatUsdMicros(1_230_000)).toBe('$1.23')
  })

  it('formats zero as $0.00', () => {
    expect(formatUsdMicros(0)).toBe('$0.00')
  })
})

describe('isLocalOnly', () => {
  it('is true only when every provider is local', () => {
    expect(isLocalOnly(['local'])).toBe(true)
    expect(isLocalOnly(['local', 'local'])).toBe(true)
  })

  it('is false when any provider is a cloud one, or the list is empty', () => {
    expect(isLocalOnly(['local', 'openai'])).toBe(false)
    expect(isLocalOnly(['anthropic'])).toBe(false)
    expect(isLocalOnly([])).toBe(false)
  })
})

describe('costReading', () => {
  const metered = (over: Partial<ConversationCost>): ConversationCost => ({
    promptTokens: 1_000,
    completionTokens: 200,
    costMicros: 1_500,
    fullyPriced: true,
    providers: ['anthropic'],
    ...over,
  })
  const empty = metered({ promptTokens: 0, completionTokens: 0, costMicros: 0, providers: [] })
  const cloud = { provider: 'anthropic' as const, priced: true }

  it('reads a metered thread from the meter, whatever the next call would use', () => {
    expect(costReading(metered({}), cloud)).toEqual({ tokens: 1_200, price: { kind: 'estimate', micros: 1_500 } })
    expect(costReading(metered({ providers: ['local'] }), cloud)).toEqual({ tokens: 1_200, price: { kind: 'free' } })
    expect(costReading(metered({ fullyPriced: false }), cloud)).toEqual({ tokens: 1_200, price: { kind: 'unknown' } })
  })

  it('reads an unmetered thread as zero, priced the way its first call would be', () => {
    // A brand-new chat shows "0 tokens" plus an honest word, never a silent $0 for an unpriced model.
    expect(costReading(null, cloud)).toEqual({ tokens: 0, price: { kind: 'estimate', micros: 0 } })
    expect(costReading(empty, { provider: 'local', priced: true })).toEqual({ tokens: 0, price: { kind: 'free' } })
    expect(costReading(empty, { provider: 'openAi', priced: false })).toEqual({ tokens: 0, price: { kind: 'unknown' } })
  })

  it('names no price while AI is off: there is nothing to price', () => {
    expect(costReading(null, null)).toEqual({ tokens: 0, price: null })
  })
})
