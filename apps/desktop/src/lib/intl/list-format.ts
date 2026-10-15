/**
 * Locale-aware "A, B, and C" (and "A, B, or C") joining, memoized.
 *
 * The one place a list of names becomes a phrase inside a sentence, so no
 * feature module hand-joins with `', '` and a hardcoded English "and". The
 * callers: the eject refusal that names the apps still holding a drive
 * (`#lib/file-explorer/navigation/eject-error-messages.ts`), and the transfer
 * toast naming the metadata a destination couldn't keep
 * (`#lib/file-operations/transfer/transfer-complete-toast.ts`).
 *
 * The locale is {@link getUiLocale}, not the OS formatting one: this is a
 * conjunction inside catalog copy, so it has to speak the language the sentence
 * around it speaks. That's the split `./locale.ts` draws, and it's why sizes and
 * dates resolve the other way.
 *
 * Memoized per locale for the same reason `number-format.ts` is: constructing an
 * `Intl` formatter costs far more than formatting with one.
 */

import { getUiLocale } from './locale'

interface ListJoiner {
  formatter: Intl.ListFormat
  /** Whether the joins need Han–Latin spacing on top of CLDR's pattern. */
  spacesHan: boolean
}

type ListType = 'conjunction' | 'disjunction'

/** Cache of one joiner per UI locale and list type. */
const listFormatterCache = new Map<string, ListJoiner>()

/** A memoized `Intl.ListFormat` of `type` for the active UI locale. */
function getJoiner(type: ListType): ListJoiner {
  const locale = getUiLocale()
  const cacheKey = `${type}:${locale}`
  let joiner = listFormatterCache.get(cacheKey)
  if (joiner === undefined) {
    joiner = {
      formatter: new Intl.ListFormat(locale, { style: 'long', type }),
      spacesHan: spacesHanAgainstLatin(locale),
    }
    listFormatterCache.set(cacheKey, joiner)
  }
  return joiner
}

/**
 * Join names the way the UI language joins them: `Preview and Warp`,
 * `Preview, Warp, and Photos`. An empty list answers an empty string, which is
 * the caller's cue that it has no list sentence to say.
 */
export function formatConjunctionList(items: readonly string[]): string {
  return formatList(items, 'conjunction')
}

/**
 * Join alternatives the way the UI language says "or": `dates or tags`,
 * `dates, tags, or flags`. Empty for an empty list, like
 * {@link formatConjunctionList}.
 */
export function formatDisjunctionList(items: readonly string[]): string {
  return formatList(items, 'disjunction')
}

/**
 * List standalone facts with the UI language's enumeration comma and no "and":
 * `Photos, copying, 3 of 10 files`, `写真、コピー中`. For an accessible name built
 * from phrases that each read on their own (a transfer tree row's, a file row's).
 * Empty for an empty list.
 *
 * Not CLDR's `unit` list: that one ends "B und C" in German (and "y", "et", "és"
 * elsewhere) and runs items together with no separator at all in Chinese. Both
 * break an accessible name that has to carry the row's visible words in order
 * (WCAG 2.5.3). Every language's MIDDLE conjunction separator is punctuation, so
 * every join uses that one.
 */
export function formatUnitList(items: readonly string[]): string {
  return items.join(enumerationSeparator())
}

/** Cache of the enumeration comma per UI locale. */
const separatorCache = new Map<string, string>()

/** The literal CLDR puts between the first two of three conjunction items: `, ` or `、`. */
function enumerationSeparator(): string {
  const locale = getUiLocale()
  let separator = separatorCache.get(locale)
  if (separator === undefined) {
    const parts = new Intl.ListFormat(locale, { style: 'long', type: 'conjunction' }).formatToParts(['a', 'b', 'c'])
    separator = parts.find((part) => part.type === 'literal')?.value ?? ', '
    separatorCache.set(locale, separator)
  }
  return separator
}

function formatList(items: readonly string[], type: ListType): string {
  if (items.length === 0) return ''
  const { formatter, spacesHan } = getJoiner(type)
  if (!spacesHan) return formatter.format(items)
  return formatter
    .formatToParts(items)
    .map((part, i, parts) =>
      part.type === 'literal' ? spaceLiteral(part.value, parts[i - 1]?.value, parts[i + 1]?.value) : part.value,
    )
    .join('')
}

const HAN = /\p{Script=Han}/u
const LATIN_OR_DIGIT = /[\p{Script=Latin}\p{Nd}]/u

/**
 * Chinese puts a space between Han and a Latin word or number, and CLDR's list
 * patterns don't: `Warp和其他 App`. Both our Chinese style guides rule the
 * spaced form (`docs/i18n/zh/style.md`, `docs/i18n/zh-Hant/style.md`), and the
 * names joined here are app names, often Latin. Japanese runs tight, so this is
 * by language, not by script.
 */
function spacesHanAgainstLatin(locale: string): boolean {
  return locale.split('-')[0].toLowerCase() === 'zh'
}

/** Pads a connective like `和` where a Han edge meets a Latin one. The
 *  enumeration comma `、` is full-width punctuation, so it never gets one. */
function spaceLiteral(literal: string, before: string | undefined, after: string | undefined): string {
  const lead = HAN.test(literal.at(0) ?? '') && LATIN_OR_DIGIT.test(before?.at(-1) ?? '') ? ' ' : ''
  const trail = HAN.test(literal.at(-1) ?? '') && LATIN_OR_DIGIT.test(after?.at(0) ?? '') ? ' ' : ''
  return lead + literal + trail
}

/** Test seam: drop the memoization cache so a memoization assertion starts clean. */
export function _clearListFormatCacheForTests(): void {
  listFormatterCache.clear()
  separatorCache.clear()
}
