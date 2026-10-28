/**
 * Pure cost-formatting helpers for the per-thread footer and the settings spend display.
 *
 * Costs are integer micro-USD estimates from the backend meter (providers bill in USD, so
 * the currency is USD regardless of the display locale). The honesty rules live in
 * `costReading`: a local-only thread reads "free, on-device", an unpriced turn reads "cost
 * unknown", and only a fully-priced thread shows an estimated amount — never a silent $0.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger, formatWorded, getNumberFormatter } from '#lib/intl/number-format.js'
import type { ConversationCost, NextCallPricing } from '#lib/tauri-commands/index.js'

/** Total tokens (prompt + completion) for a metered record. */
export function totalTokens(cost: { promptTokens: number; completionTokens: number }): number {
  return cost.promptTokens + cost.completionTokens
}

/** From here up, a token count reads compact ("2k", "1.3M"); below it, exact. */
const COMPACT_TOKENS_FROM = 2_000

/**
 * A token count as the rail shows it: exact below 2,000, then thousands ("2k", "2.2k") and
 * millions ("1M", "1.3M") with one decimal at most. Rounding picks the unit, so 999,950 reads
 * "1M", never "1,000k". The decimal mark follows the formatting locale like every number; the
 * "k" / "M" suffix is the UI language's (a catalog string). Exact counts belong in a tooltip.
 */
export function formatTokenCount(tokens: number): string {
  if (tokens < COMPACT_TOKENS_FROM) return formatInteger(tokens)
  const oneDecimal = getNumberFormatter({ maximumFractionDigits: 1 })
  const thousands = Math.round(tokens / 100) / 10
  if (thousands < 1_000) return tString('askCmdr.tokens.thousands', { amount: oneDecimal.format(thousands) })
  const millions = Math.round(tokens / 100_000) / 10
  return tString('askCmdr.tokens.millions', { amount: oneDecimal.format(millions) })
}

/**
 * Format an integer micro-USD estimate as a USD amount rounded to cents ("$0.83"): the narrow
 * symbol and its place from the UI language, the digits from the formatting locale
 * (`formatWorded`). The tooltip beside it says it's USD, so the symbol stays narrow.
 */
export function formatUsdMicros(micros: number): string {
  const options = {
    style: 'currency',
    currency: 'USD',
    currencyDisplay: 'narrowSymbol',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  } as const
  return formatWorded(options, micros / 1_000_000)
}

/** Half a cent in micro-USD: anything below it would round to `$0.00`. */
const HALF_CENT_MICROS = 5_000

/**
 * The estimate as a phrase: "about $0.83", or "under $0.01" for a real cost too small to show
 * in cents (a short chat often costs a fraction of a cent, and "$0.00" would read as free).
 */
export function costEstimateText(micros: number): string {
  if (micros > 0 && micros < HALF_CENT_MICROS) {
    return tString('askCmdr.cost.underAmount', { amount: formatUsdMicros(10_000) })
  }
  return tString('askCmdr.cost.estimate', { amount: formatUsdMicros(micros) })
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
