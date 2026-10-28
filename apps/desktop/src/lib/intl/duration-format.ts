/**
 * Locale-aware compact durations ("8m 12s", "8min 12s", "8 Min., 12 Sek."),
 * memoized.
 *
 * The output is what `Intl.DurationFormat` gives with `style: 'narrow'`, built
 * from a unit-style `Intl.NumberFormat` per unit plus a unit-type
 * `Intl.ListFormat` to join them. Cmdr can't call `Intl.DurationFormat` itself:
 * it arrived in Safari 16.4, and the app's floor is older (the `safari15` build
 * target, macOS 10.15). The two pieces used here are what the standard composes
 * internally, and `duration.test.ts` pins the result against it.
 *
 * The locale is {@link getCatalogLocale}, the catalog tag the UI speaks: "min"
 * and "Sek." are words in the sentence around them, and the digits are
 * numbers, which follow the UI language too.
 */

import { createListFormat } from './list-format'
import { getCatalogLocale } from './locale'
import { getNumberFormatter } from './number-format'

/** The units a compact duration can name. */
export type DurationUnit = 'hour' | 'minute' | 'second'

/** One duration component: a whole number of one unit. */
export interface DurationPart {
  unit: DurationUnit
  value: number
}

const unitFormatterCache = new Map<string, Intl.NumberFormat>()
/** `undefined` per locale on an engine without `Intl.ListFormat` (Catalina). */
const joinerCache = new Map<string, Intl.ListFormat | undefined>()

function getUnitFormatter(locale: string, unit: DurationUnit): Intl.NumberFormat {
  const key = `${locale} ${unit}`
  let formatter = unitFormatterCache.get(key)
  if (formatter === undefined) {
    formatter = new Intl.NumberFormat(locale, { style: 'unit', unit, unitDisplay: 'narrow' })
    unitFormatterCache.set(key, formatter)
  }
  return formatter
}

function getJoiner(locale: string): Intl.ListFormat | undefined {
  if (!joinerCache.has(locale)) joinerCache.set(locale, createListFormat(locale, { type: 'unit', style: 'narrow' }))
  return joinerCache.get(locale)
}

/**
 * Formats duration components, largest first, in the UI language's narrow
 * style. The caller decides which components to show (dropping a zero tail,
 * say); this only words them. Without `Intl.ListFormat` the units join with a
 * space, CLDR's narrow pattern for English.
 */
export function formatNarrowDuration(parts: readonly DurationPart[]): string {
  const locale = getCatalogLocale()
  const worded = parts.map(({ unit, value }) => getUnitFormatter(locale, unit).format(value))
  return getJoiner(locale)?.format(worded) ?? worded.join(' ')
}

/** The units a single measured timing can be worded in. */
export type MeasureUnit = 'millisecond' | 'second'

/**
 * Words one timing ("847ms", "1.4s", "1,4 Sek.") in the UI language's narrow
 * style, with `fractionDigits` digits after the decimal mark.
 */
export function formatNarrowMeasure(unit: MeasureUnit, value: number, fractionDigits: number): string {
  return getNumberFormatter({
    style: 'unit',
    unit,
    unitDisplay: 'narrow',
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(value)
}
