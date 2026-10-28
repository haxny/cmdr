/**
 * Unit tests for the pure cost-formatting helpers (the per-thread footer + settings spend).
 * `formatUsdMicros` reads the locale via the intl layer; these assert en-US output.
 */

import { describe, it, expect, beforeAll, afterEach } from 'vitest'
import { _setFormatLocaleForTests, _setLocaleForTests } from '#lib/intl/locale.js'
import type { ConversationCost } from '#lib/tauri-commands/index.js'
import {
  costEstimateText,
  costReading,
  formatTokenCount,
  formatUsdMicros,
  isLocalOnly,
  totalTokens,
} from './ask-cmdr-cost'

beforeAll(() => {
  _setLocaleForTests('en-US')
})

describe('totalTokens', () => {
  it('sums prompt and completion tokens', () => {
    expect(totalTokens({ promptTokens: 300, completionTokens: 70 })).toBe(370)
  })
})

describe('formatUsdMicros', () => {
  afterEach(() => {
    _setLocaleForTests('en-US')
  })

  it('rounds to cents', () => {
    // 825,600 micro-USD = $0.8256, the amount David's QA screenshot showed as "0,8256 US$".
    expect(formatUsdMicros(825_600)).toBe('$0.83')
    expect(formatUsdMicros(1_230_000)).toBe('$1.23')
    expect(formatUsdMicros(1_234_560_000)).toBe('$1,234.56')
  })

  it('formats zero as $0.00', () => {
    expect(formatUsdMicros(0)).toBe('$0.00')
  })

  it('reads entirely in the app language, whatever the OS region', () => {
    // English app on a Swedish-formatted Mac: the region alone would write "0,83 US$".
    _setLocaleForTests('en')
    _setFormatLocaleForTests('en-SE')
    expect(formatUsdMicros(825_600)).toBe('$0.83')
    _setLocaleForTests('de')
    expect(formatUsdMicros(825_600).replace(/\s/g, ' ')).toBe('0,83 $')
  })
})

describe('costEstimateText', () => {
  it('reads "about" a rounded amount', () => {
    expect(costEstimateText(825_600)).toBe('about $0.83')
    expect(costEstimateText(0)).toBe('about $0.00')
  })

  it('never rounds a real sub-cent cost to $0.00', () => {
    expect(costEstimateText(400)).toBe('under $0.01')
    expect(costEstimateText(4_999)).toBe('under $0.01')
    expect(costEstimateText(5_000)).toBe('about $0.01')
  })
})

describe('formatTokenCount', () => {
  it('keeps exact counts below 2,000', () => {
    expect(formatTokenCount(0)).toBe('0')
    expect(formatTokenCount(1_999)).toBe('1,999')
  })

  it('shows thousands with one decimal at most', () => {
    expect(formatTokenCount(2_000)).toBe('2k')
    expect(formatTokenCount(2_240)).toBe('2.2k')
    expect(formatTokenCount(194_776)).toBe('194.8k')
    expect(formatTokenCount(999_949)).toBe('999.9k')
  })

  it('switches to millions where thousands would round to 1,000', () => {
    expect(formatTokenCount(999_950)).toBe('1M')
    expect(formatTokenCount(1_000_000)).toBe('1M')
    expect(formatTokenCount(1_340_000)).toBe('1.3M')
  })

  it('takes its decimal mark from the app language, not the OS region', () => {
    _setLocaleForTests('en')
    _setFormatLocaleForTests('en-SE')
    expect(formatTokenCount(2_240)).toBe('2.2k')
    _setLocaleForTests('de')
    expect(formatTokenCount(2_240).replace(/\s/g, ' ')).toBe('2,2 Tsd.')
    _setLocaleForTests('en-US')
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
