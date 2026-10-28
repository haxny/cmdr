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
 * The digits always come from {@link getFormatLocale}, which follows the OS
 * rather than the UI-language setting; this module never resolves a locale
 * itself. {@link formatWorded} is the one place the UI language joins in: it
 * takes the words around the number (a unit, a currency symbol) and their
 * placement from the UI language, and the digits from the OS.
 */

import { getFormatLocale, getUiLocale } from './locale'

/** Cache keyed by `${locale} ${JSON.stringify(options)}`. */
const formatterCache = new Map<string, Intl.NumberFormat>()

function cachedFormatter(locale: string, options: Intl.NumberFormatOptions): Intl.NumberFormat {
  const key = `${locale} ${JSON.stringify(options)}`
  let formatter = formatterCache.get(key)
  if (formatter === undefined) {
    formatter = new Intl.NumberFormat(locale, options)
    formatterCache.set(key, formatter)
  }
  return formatter
}

/**
 * A memoized `Intl.NumberFormat` for the active locale and the given options.
 * Callers pass the options they need (fraction digits, grouping); identical
 * (locale, options) pairs share one instance.
 */
export function getNumberFormatter(options: Intl.NumberFormatOptions): Intl.NumberFormat {
  return cachedFormatter(getFormatLocale(), options)
}

/** `formatToParts` types that make up the number itself, as opposed to the words around it. */
const NUMBER_PART_TYPES = new Set(['integer', 'group', 'decimal', 'fraction', 'minusSign'])

/**
 * Formats a number with words around it (`style: 'unit'` or `'currency'`), splitting the job
 * the way a size does: the words and where they sit come from the UI language, the digits
 * (decimal mark, grouping) from the formatting locale. A French speaker on a US-formatted Mac
 * reads "1.4 s"; an English speaker on a Swedish-formatted Mac reads "$0,83", where the
 * formatting locale alone would write "0,83 US$".
 *
 * Pass explicit `minimumFractionDigits` / `maximumFractionDigits`: the digits formatter sees
 * only those, not the style's defaults.
 */
export function formatWorded(options: Intl.NumberFormatOptions, value: number): string {
  const digits = getNumberFormatter({
    minimumFractionDigits: options.minimumFractionDigits,
    maximumFractionDigits: options.maximumFractionDigits,
  }).format(value)
  let numberWritten = false
  return cachedFormatter(getUiLocale(), options)
    .formatToParts(value)
    .map((part) => {
      if (!NUMBER_PART_TYPES.has(part.type)) return part.value
      if (numberWritten) return ''
      numberWritten = true
      return digits
    })
    .join('')
}

/**
 * Format an integer count with locale-aware thousands grouping (e.g. `1,234`
 * in en-US, `1.234` in de-DE). The single helper behind `formatNumber` and
 * every other user-facing count readout.
 */
export function formatInteger(n: number): string {
  return getNumberFormatter({ maximumFractionDigits: 0 }).format(n)
}

/**
 * Format a money amount in its own ISO 4217 currency (`USD`, `EUR`), rounded to
 * that currency's minor unit, in the formatting locale (`$0.02` in en-US,
 * `0,02 $` in de-DE). Its formatter's `resolvedOptions().maximumFractionDigits`
 * is that minor unit, for a caller deciding whether an amount shows as zero.
 */
export function formatMoney(amount: number, currency: string): string {
  return getNumberFormatter({ style: 'currency', currency }).format(amount)
}

/** Cache for the per-locale grouping separator. */
const groupSeparatorCache = new Map<string, string>()

/**
 * The active locale's thousands-group separator (e.g. `,` in en-US, `.` in
 * de-DE, a thin/narrow no-break space in fr-FR). Derived from the same
 * `Intl.NumberFormat` the counts use, so byte-triad grouping agrees with
 * counts. Memoized per locale.
 */
export function getGroupSeparator(): string {
  const locale = getFormatLocale()
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
