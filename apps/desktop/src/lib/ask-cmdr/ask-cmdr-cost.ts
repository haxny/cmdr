/**
 * Pure cost-formatting helpers for the per-thread footer and the settings spend display.
 *
 * Costs are integer micro-USD estimates from the backend meter (providers bill in USD, so
 * the currency is USD regardless of the display locale). The honesty rules live in
 * `costReading`: a local-only thread reads "free, on-device", an unpriced turn reads "cost
 * unknown", and only a fully-priced thread shows an estimated amount — never a silent $0.
 */

import { getNumberFormatter } from '#lib/intl/number-format.js'
import type { ConversationCost, NextCallPricing } from '#lib/tauri-commands/index.js'

/** Total tokens (prompt + completion) for a metered record. */
export function totalTokens(cost: { promptTokens: number; completionTokens: number }): number {
  return cost.promptTokens + cost.completionTokens
}

/**
 * Format an integer micro-USD estimate as a locale-aware USD amount. A sub-dollar amount
 * keeps up to four fraction digits (a chat often costs a few tenths of a cent, and rounding
 * that to `$0.00` would read as free); a dollar or more rounds to cents.
 */
export function formatUsdMicros(micros: number): string {
  const dollars = micros / 1_000_000
  const maximumFractionDigits = Math.abs(dollars) >= 1 ? 2 : 4
  return getNumberFormatter({ style: 'currency', currency: 'USD', maximumFractionDigits }).format(dollars)
}

/** True when every metered turn used a local/on-device model (so the thread is free). */
export function isLocalOnly(providers: string[]): boolean {
  return providers.length > 0 && providers.every((provider) => provider === 'local')
}

/** The footer's cost line as data: a token count and, when there is something to price, how. */
export type CostReading = {
  tokens: number
  price: { kind: 'free' } | { kind: 'unknown' } | { kind: 'estimate'; micros: number } | null
}

/**
 * What the footer says about money. A thread with metered calls reads from the meter (the one
 * source of its total). A thread with none (a brand-new chat, or one whose turns all failed)
 * reads zero, priced the way its first call would be (`nextCall`, from the slot status), so it
 * says "free", "about $0", or "cost unknown" honestly from the start. `nextCall` is null while
 * AI is off, and then nothing is priced at all.
 */
export function costReading(threadCost: ConversationCost | null, nextCall: NextCallPricing | null): CostReading {
  if (threadCost && totalTokens(threadCost) > 0) {
    if (isLocalOnly(threadCost.providers)) return { tokens: totalTokens(threadCost), price: { kind: 'free' } }
    if (!threadCost.fullyPriced) return { tokens: totalTokens(threadCost), price: { kind: 'unknown' } }
    return { tokens: totalTokens(threadCost), price: { kind: 'estimate', micros: threadCost.costMicros } }
  }
  if (!nextCall) return { tokens: 0, price: null }
  if (nextCall.provider === 'local') return { tokens: 0, price: { kind: 'free' } }
  if (!nextCall.priced) return { tokens: 0, price: { kind: 'unknown' } }
  return { tokens: 0, price: { kind: 'estimate', micros: 0 } }
}
