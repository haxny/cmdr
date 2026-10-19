/**
 * The one place a logged value becomes text.
 *
 * The log bridge renders every message part through this, so a call site passes the raw value
 * (`{ error: e }`) and never pre-stringifies it: `String(e)` turns a plain object into
 * `[object Object]` and an `Error` into a bare `Name: message` with its cause dropped
 * (ERR-VEZUB arrived as `detail="[object Object]"`). `cmdr/no-stringified-log-values` keeps
 * call sites from doing it anyway.
 *
 * Objects serialize as COMPACT JSON, with the value of every identity-keyed pair (`"path"`,
 * `"name"`, `"server"`, …) marked by its kind (`log-markers.ts`), so an error report hashes
 * exactly that value. Every other char of the rendering that falls in the marker range is
 * replaced: anything arriving over IPC is UI data, and a reserved char in it is a hostile name.
 */

import { TypedFailure } from '#lib/ipc/typed-failure.js'
import {
  LOG_MARKER_CLOSER,
  type LogKind,
  isReservedChar,
  logKindOfJsonKey,
  markLogValue,
  withoutReserved,
} from './log-markers'

/**
 * Longest rendering of one value, about the 1 KiB the Rust side's `LogDetail` keeps. A report
 * caps a `detail` field far shorter; this bounds the local log against a runaway object.
 */
export const MAX_DESCRIBED_CHARS = 1000

/** How many causes deep the chain goes, against a pathological (non-cyclic) chain. */
const MAX_CAUSE_DEPTH = 8

interface DescribeOptions {
  /**
   * Mark identity-keyed JSON values (the default). Off for a value the bridge marks whole as one
   * leaf, where inner spans would only be escaped into its text.
   */
  marks?: boolean
}

export function describeForLog(value: unknown, { marks = true }: DescribeOptions = {}): string {
  if (typeof value === 'string') return withoutReserved(value)
  if (typeof value !== 'object' || value === null) return withoutReserved(String(value))
  return cap(value instanceof Error ? describeError(value, marks) : describeObject(value, marks))
}

/** `Name: message`, a `TypedFailure`'s payload, then the cause chain. */
function describeError(error: Error, marks: boolean): string {
  const seen = new Set<unknown>()
  const links: string[] = []
  let current: unknown = error
  while (current != null && !seen.has(current) && links.length < MAX_CAUSE_DEPTH) {
    seen.add(current)
    if (!(current instanceof Error)) {
      links.push(describeObjectOrPrimitive(current, marks))
      break
    }
    links.push(errorHeadline(current, marks))
    current = (current as { cause?: unknown }).cause
  }
  return links.join('; caused by ')
}

function errorHeadline(error: Error, marks: boolean): string {
  const headline = withoutReserved(error.message ? `${error.name}: ${error.message}` : error.name)
  return error instanceof TypedFailure ? `${headline} ${toJson(error.failure, marks)}` : headline
}

function describeObjectOrPrimitive(value: unknown, marks: boolean): string {
  if (typeof value === 'string') return withoutReserved(value)
  if (typeof value !== 'object' || value === null) return withoutReserved(String(value))
  return describeObject(value, marks)
}

/** A class with its own `toString` (`URL`, `Date`) says it best; anything else goes as JSON. */
function describeObject(value: object, marks: boolean): string {
  const toString = Array.isArray(value) ? undefined : ownToString(value)
  return toString ? withoutReserved(String(toString.call(value))) : toJson(value, marks)
}

function ownToString(value: object): ((this: object) => unknown) | undefined {
  const method = (value as { toString?: unknown }).toString
  if (typeof method !== 'function' || method === Object.prototype.toString) return undefined
  return method as (this: object) => unknown
}

/**
 * `JSON.stringify` with its real return type: lib.d.ts says `string`, but a top-level `undefined`,
 * function, or symbol yields `undefined`.
 */
const stringifyJson: (
  value: unknown,
  replacer: (this: object, key: string, nested: unknown) => unknown,
) => string | undefined = JSON.stringify

/** Compact JSON with a cycle guard, identity values marked; nested errors render as text instead of `{}`. */
function toJson(value: unknown, marks: boolean): string {
  const ancestors: unknown[] = []
  try {
    const json = stringifyJson(value, function (this: object, _key, nested: unknown) {
      if (typeof nested === 'bigint') return nested.toString()
      if (typeof nested !== 'object' || nested === null) return nested
      // Unmarked: it lands inside a JSON string, where a span would read one escape layer off.
      if (nested instanceof Error) return describeError(nested, false)
      // `this` is the holder of `nested`: unwind the ancestor stack to it, so a value
      // repeated in a sibling isn't mistaken for a cycle.
      while (ancestors.length > 0 && ancestors.at(-1) !== this) ancestors.pop()
      if (ancestors.includes(nested)) return '[Circular]'
      ancestors.push(nested)
      return nested
    })
    return json === undefined ? withoutReserved(String(value)) : rewriteStrings(json, marks)
  } catch {
    // A getter that throws, or a Proxy: the value still deserves a line.
    return `[unserializable ${withoutReserved(constructorName(value))}]`
  }
}

/** A key's colon, read in place at `lastIndex`. */
const COLON_AHEAD = /\s*:/uy

/**
 * Walk the serialized JSON's string tokens: an identity-keyed string value gets marked (after
 * `JSON.stringify`, never in its replacer, whose output JSON would escape the span body a second
 * time), and every other string gets its reserved chars replaced. Reading whole tokens keeps a
 * key spelled inside a value (`"x\",\"path\":\"y"`) from passing for a pair.
 */
function rewriteStrings(json: string, marks: boolean): string {
  let out = ''
  let pos = 0
  let pendingKind: LogKind | undefined
  while (pos < json.length) {
    const open = json.indexOf('"', pos)
    if (open === -1) break
    const close = endOfJsonString(json, open)
    const between = json.slice(pos, open)
    const token = json.slice(open, close)
    COLON_AHEAD.lastIndex = close
    const isKey = COLON_AHEAD.test(json)
    // A value directly follows its key's colon; anything else between them (`[`, `{`) means the
    // value isn't a string, and the pending kind lapses.
    const valueKind = pendingKind !== undefined && /^\s*:\s*$/u.test(between) ? pendingKind : undefined
    out += between
    if (isKey) {
      out += withoutReserved(token)
      pendingKind = marks ? logKindOfJsonKey(JSON.parse(token) as string) : undefined
    } else {
      out +=
        valueKind === undefined ? withoutReserved(token) : `"${markLogValue(valueKind, JSON.parse(token) as string)}"`
      pendingKind = undefined
    }
    pos = close
  }
  return out + json.slice(pos)
}

/** Just past the closing quote of the JSON string that opens at `open`. */
function endOfJsonString(json: string, open: number): number {
  let at = open + 1
  while (at < json.length && json[at] !== '"') at += json[at] === '\\' ? 2 : 1
  return at + 1
}

function constructorName(value: unknown): string {
  const name = (value as { constructor?: { name?: unknown } } | null)?.constructor?.name
  return typeof name === 'string' && name !== '' ? name : 'object'
}

/** At most [`MAX_DESCRIBED_CHARS`], never cutting inside a span: one the cut would split goes whole. */
function cap(text: string): string {
  if (text.length <= MAX_DESCRIBED_CHARS) return text
  let cut = MAX_DESCRIBED_CHARS
  const kept = text.slice(0, cut)
  const opened = lastOpenerIndex(kept)
  if (opened !== -1 && !kept.includes(LOG_MARKER_CLOSER, opened)) cut = opened
  return `${text.slice(0, cut)}…[${String(text.length - cut)} chars]`
}

/** Where the last opener in `text` sits: our own markers are the only reserved chars left. */
function lastOpenerIndex(text: string): number {
  for (let at = text.length - 1; at >= 0; at--) {
    if (isReservedChar(text[at]) && text[at] !== LOG_MARKER_CLOSER) return at
  }
  return -1
}
