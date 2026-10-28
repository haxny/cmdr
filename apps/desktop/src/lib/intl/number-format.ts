/**
 * Locale-aware number formatting, memoized.
 *
 * `Intl.NumberFormat` construction costs ~10× a format call, and the count and
 * size formatters run per-visible-entry in render AND again in the
 * column-measurement fold over the prefetch buffer. Constructing per call would
 * regress scroll/measure performance on large directories, so we cache each
 * `Intl.NumberFormat` by (locale, options) and rebuild only when the active
 * locale changes. Mirrors the lazy-singleton shape of
 * `getSystemLocaleFormatter()` in `#lib/settings/format-utils`.
 *
 * Every number here is written in {@link getNumberLocale}: the catalog tag the
 * UI speaks, so digits, the decimal mark, grouping, and the words around a
 * number (a unit, a currency symbol) all match the sentence they sit in. Dates
 * stay on the OS region (`getFormatLocale()`); the why is `DETAILS.md` §
 * "Language and region are two settings". This module never resolves a locale
 * itself.
 */

import { getNumberLocale } from './locale'

/** Cache keyed by `${locale} ${JSON.stringify(options)}`. */
const formatterCache = new Map<string, Intl.NumberFormat>()

/**
 * A memoized `Intl.NumberFormat` for the active number locale and the given
 * options. Callers pass the options they need (fraction digits, grouping, a
 * `unit` or `currency` style); identical (locale, options) pairs share one
 * instance.
 */
export function getNumberFormatter(options: Intl.NumberFormatOptions): Intl.NumberFormat {
  const locale = getNumberLocale()
  const key = `${locale} ${JSON.stringify(options)}`
  let formatter = formatterCache.get(key)
  if (formatter === undefined) {
    formatter = new Intl.NumberFormat(locale, options)
    formatterCache.set(key, formatter)
  }
  return formatter
}

/**
 * Format an integer count with locale-aware thousands grouping (e.g. `1,234`
 * in English, `1.234` in German). The single helper behind `formatNumber` and
 * every other user-facing count readout.
 */
export function formatInteger(n: number): string {
  return getNumberFormatter({ maximumFractionDigits: 0 }).format(n)
}

/**
 * A count as a short approximate figure in the UI language's own compact form
 * (`12.3K`, `1.2M`, Chinese `2.2万`), for a readout that only needs the order of
 * magnitude. CLDR picks the unit, so a language counting in 10⁴ (zh, ja) gets
 * it, and one with no short thousands (de) writes the full count.
 */
export function formatCompactCount(n: number): string {
  return getNumberFormatter({ notation: 'compact', maximumFractionDigits: 1 }).format(n)
}

/**
 * Format a money amount in its own ISO 4217 currency (`USD`, `EUR`), rounded to
 * that currency's minor unit (`$0.02` in English, `0,02 $` in German). Its
 * formatter's `resolvedOptions().maximumFractionDigits` is that minor unit, for
 * a caller deciding whether an amount shows as zero.
 */
export function formatMoney(amount: number, currency: string): string {
  return getNumberFormatter({ style: 'currency', currency }).format(amount)
}

/** Cache for the per-locale grouping separator. */
const groupSeparatorCache = new Map<string, string>()

/**
 * The number locale's thousands-group separator (e.g. `,` in English, `.` in
 * German, a narrow no-break space in French). Derived from the same
 * `Intl.NumberFormat` the counts use, so byte-triad grouping agrees with
 * counts. Memoized per locale.
 */
export function getGroupSeparator(): string {
  const locale = getNumberLocale()
  let separator = groupSeparatorCache.get(locale)
  if (separator === undefined) {
    // 11111 is large enough to force one group boundary in every locale.
    separator = new Intl.NumberFormat(locale).formatToParts(11111).find((p) => p.type === 'group')?.value ?? ''
    groupSeparatorCache.set(locale, separator)
  }
  return separator
}

/** Test seam: drop both memoization caches so a memoization assertion starts clean. */
export function _clearCachesForTests(): void {
  formatterCache.clear()
  groupSeparatorCache.clear()
}
