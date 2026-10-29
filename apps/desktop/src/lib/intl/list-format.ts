/**
 * Locale-aware "A, B, and C" (and "A, B, or C") joining, memoized.
 *
 * The one place a list of names becomes a phrase inside a sentence, so no
 * feature module hand-joins with `', '` and a hardcoded English "and". The
 * callers: the eject refusal that names the apps still holding a drive
 * (`#lib/file-explorer/navigation/eject-error-messages.ts`), the transfer
 * toast naming the metadata a destination couldn't keep
 * (`#lib/file-operations/transfer/transfer-complete-toast.ts`), and the Ask Cmdr
 * tool line listing the folders a search looked in, as links
 * (`conjunctionListParts`).
 *
 * The locale is {@link getCatalogLocale}, the RESOLVED catalog tag: this is a
 * conjunction inside catalog copy, so it has to speak the language the sentence
 * around it is actually written in (a `ja-JP` Mac with no `ja` catalog reads
 * English, so it joins with "and"). Dates resolve the other way (`./locale.ts`).
 *
 * Memoized per locale for the same reason `number-format.ts` is: constructing an
 * `Intl` formatter costs far more than formatting with one.
 *
 * `Intl.ListFormat` can be missing entirely (every Catalina, whatever its Safari),
 * so nothing here constructs one directly: {@link createListFormat} answers
 * `undefined` there, and each join falls back to the enumeration comma. Why the
 * engine lacks it: `DETAILS.md` § Catalina has no Intl.ListFormat.
 */

import { getCatalogLocale } from './locale'

interface ListJoiner {
  /** `undefined` on an engine without `Intl.ListFormat`. */
  formatter: Intl.ListFormat | undefined
  /** Whether the joins need Han–Latin spacing on top of CLDR's pattern. */
  spacesHan: boolean
}

type ListType = 'conjunction' | 'disjunction'

/**
 * A new `Intl.ListFormat`, or `undefined` where the engine has none. The one
 * place in the app that calls the constructor (`duration-format.ts` comes here too).
 */
export function createListFormat(locale: string, options: Intl.ListFormatOptions): Intl.ListFormat | undefined {
  // The lib types say it always exists; Catalina's JavaScriptCore says otherwise.
  if (typeof (Intl as { ListFormat?: unknown }).ListFormat !== 'function') return undefined
  // eslint-disable-next-line no-restricted-properties -- the feature-detected door itself
  return new Intl.ListFormat(locale, options)
}

/** Cache of one joiner per UI locale and list type. */
const listFormatterCache = new Map<string, ListJoiner>()

/** A memoized `Intl.ListFormat` of `type` for the active UI locale. */
function getJoiner(type: ListType): ListJoiner {
  const locale = getCatalogLocale()
  const cacheKey = `${type}:${locale}`
  let joiner = listFormatterCache.get(cacheKey)
  if (joiner === undefined) {
    joiner = {
      formatter: createListFormat(locale, { style: 'long', type }),
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

/** One piece of a joined list: an item, by its index in the input, or the words between items. */
export type ListPart = { kind: 'item'; index: number } | { kind: 'literal'; text: string }

/**
 * {@link formatConjunctionList} as pieces, for a list whose items render as something other
 * than text (the Ask Cmdr tool line's path links). Joined back, the pieces read exactly as the
 * string join does, connectives and Han spacing included. Empty for an empty list.
 */
export function conjunctionListParts(items: readonly string[]): ListPart[] {
  if (items.length === 0) return []
  const { formatter, spacesHan } = getJoiner('conjunction')
  if (formatter === undefined) {
    const separator = enumerationSeparator()
    return items.flatMap((_, index): ListPart[] =>
      index === 0
        ? [{ kind: 'item', index }]
        : [
            { kind: 'literal', text: separator },
            { kind: 'item', index },
          ],
    )
  }
  const parts = formatter.formatToParts(items)
  let next = 0
  return parts.map((part, i): ListPart => {
    if (part.type === 'element') return { kind: 'item', index: next++ }
    const text = spacesHan ? spaceLiteral(part.value, parts[i - 1]?.value, parts[i + 1]?.value) : part.value
    return { kind: 'literal', text }
  })
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
  const locale = getCatalogLocale()
  let separator = separatorCache.get(locale)
  if (separator === undefined) {
    const parts = createListFormat(locale, { style: 'long', type: 'conjunction' })?.formatToParts(['a', 'b', 'c'])
    separator = parts?.find((part) => part.type === 'literal')?.value ?? fallbackEnumerationSeparator(locale)
    separatorCache.set(locale, separator)
  }
  return separator
}

/**
 * CLDR's enumeration comma without `Intl.ListFormat` to ask: the full-width `、`
 * for Chinese and Japanese, `, ` for every other language we ship.
 */
function fallbackEnumerationSeparator(locale: string): string {
  const language = locale.split('-')[0].toLowerCase()
  return language === 'zh' || language === 'ja' ? '、' : ', '
}

function formatList(items: readonly string[], type: ListType): string {
  if (items.length === 0) return ''
  const { formatter, spacesHan } = getJoiner(type)
  // No "and"/"or" word without CLDR's patterns: a comma list keeps every name, in
  // order, in any language, where a guessed English connective would not.
  if (formatter === undefined) return formatUnitList(items)
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
