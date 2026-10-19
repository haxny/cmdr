/**
 * Log markers: the frontend half of `cmdr_fs::log_private`.
 *
 * A user-derived value in a log line (a path, a name, a volume label, a host, an account name)
 * goes between its kind's opener and the closer, so an error report hashes exactly that value
 * instead of guessing where it ends. The chars are Unicode noncharacters from one reserved range,
 * listed in `log-markers.json`, which `redact/fe_bridge_tests.rs` pins to the Rust table.
 *
 * A span's value is written as the body of Rust's `{:?}`, so the same name hashes the same from
 * either side, and a hostile name (one holding a marker, a quote, a newline, a bidi override)
 * can't close its span early, split the line, or reorder a viewer's display. Every char outside a
 * span that falls in the reserved range is replaced, so only our own markers are ever structure.
 *
 * A marked string is log text, never data: ❌ never show it, store it, or send it anywhere but
 * the log bridge.
 */

import table from './log-markers.json'

/** What a marked value is. The names are `LogPrivate`'s constructors. */
export type LogKind = keyof typeof table.openers

const fromHex = (hex: string): string => String.fromCodePoint(Number.parseInt(hex, 16))

const RESERVED_FIRST = Number.parseInt(table.reserved.first, 16)
const RESERVED_LAST = Number.parseInt(table.reserved.last, 16)

/** Ends a span of any kind. */
export const LOG_MARKER_CLOSER = fromHex(table.closer)

const OPENERS = Object.fromEntries(Object.entries(table.openers).map(([kind, hex]) => [kind, fromHex(hex)])) as Record<
  LogKind,
  string
>

const RESERVED_CLASS = `${String.fromCodePoint(RESERVED_FIRST)}-${String.fromCodePoint(RESERVED_LAST)}`
const RESERVED_RE = new RegExp(`[${RESERVED_CLASS}]`, 'gu')
const HAS_RESERVED_RE = new RegExp(`[${RESERVED_CLASS}]`, 'u')
/** U+FFFD, what a reserved char outside a span becomes. */
const REPLACEMENT_CHAR = String.fromCodePoint(0xfffd)
/** An escaped marker: its backslash run, and an escape that decodes into the reserved range. */
const ESCAPED_MARKER_RE = /(\\+)u\{0{0,2}fd[d-e][0-9a-f]\}/giu
/** The chars that reorder how a line displays, the set Rust's `{:?}` escapes as `\u{…}`. */
const BIDI_FORMAT_RE = /[\u061c\u200e\u200f\u202a-\u202e\u2066-\u2069]/u

/**
 * The kind of value an identity key holds in a logged object, across the typed errors the
 * backend sends (`WriteOperationError`, `MountError`, …), or `undefined` for any other key. An
 * ID (`volumeId`, `serverId`, `deviceId`) has no kind: the report redactor tokenizes it by key
 * alone (`src-tauri/src/redact/detail.rs` `redact_json_identity_pairs`, which also knows every
 * key here, for a pair logged unmarked).
 */
export function logKindOfJsonKey(key: string): LogKind | undefined {
  return Object.hasOwn(table.jsonKeyKinds, key) ? (table.jsonKeyKinds as Record<string, LogKind>)[key] : undefined
}

/** `value` marked as `kind`: opener, `{:?}` body, closer. */
export function markLogValue(kind: LogKind, value: string): string {
  return `${OPENERS[kind]}${debugBody(value)}${LOG_MARKER_CLOSER}`
}

/** Quote like Rust's `{:?}`, which is the escaping the redactor's quoted-value grammar reads. */
export function debugQuote(value: string): string {
  return `"${debugBody(value)}"`
}

/**
 * What Rust's `{:?}` writes between its quotes, on every char that keeps a span safe: `"`, `\`,
 * control chars, line and paragraph separators, bidi-format chars, and the reserved range, in
 * Rust's spelling (`\u{…}`, lowercase hex, no padding). Rust escapes more (combining marks, its
 * other non-printables); those unescape to the same value either way.
 */
export function debugBody(value: string): string {
  let out = ''
  for (const char of value) {
    const code = char.codePointAt(0) ?? 0
    if (char === '"' || char === '\\') out += `\\${char}`
    else if (char === '\n') out += '\\n'
    else if (char === '\r') out += '\\r'
    else if (char === '\t') out += '\\t'
    else if (char === '\0') out += '\\0'
    else if (needsUnicodeEscape(char, code)) out += `\\u{${code.toString(16)}}`
    else out += char
  }
  return out
}

function needsUnicodeEscape(char: string, code: number): boolean {
  return (
    code < 0x20 ||
    (code >= 0x7f && code <= 0x9f) ||
    code === 0x2028 ||
    code === 0x2029 ||
    (code >= RESERVED_FIRST && code <= RESERVED_LAST) ||
    BIDI_FORMAT_RE.test(char)
  )
}

/** Whether `char` is in the reserved range. */
export function isReservedChar(char: string): boolean {
  const code = char.codePointAt(0) ?? 0
  return code >= RESERVED_FIRST && code <= RESERVED_LAST
}

/** `text` with every reserved char replaced by U+FFFD, so none of it can pass for a marker. */
export function withoutReserved(text: string): string {
  return mayHoldRawMarker(text) ? text.replace(RESERVED_RE, REPLACEMENT_CHAR) : text
}

/**
 * `text` with every marker gone, raw or escaped at any `{:?}` depth, for a sink a person reads.
 * An escaped marker takes only its own backslashes: at depth `d` that's `2^(d-1)`, and the rest
 * of the run belongs to the value (`cmdr_fs::log_private::markers` reads it the same way).
 */
export function stripLogMarkers(text: string): string {
  if (!mayHoldRawMarker(text) && !/u\{/u.test(text)) return text
  return text.replace(RESERVED_RE, '').replace(ESCAPED_MARKER_RE, (_match, run: string) => {
    const own = run.length & -run.length
    return run.slice(own)
  })
}

function mayHoldRawMarker(text: string): boolean {
  return HAS_RESERVED_RE.test(text)
}
