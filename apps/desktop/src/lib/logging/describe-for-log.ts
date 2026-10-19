/**
 * The one place a logged value becomes text.
 *
 * The log bridge renders every message part through this, so a call site passes the raw value
 * (`{ error: e }`) and never pre-stringifies it: `String(e)` turns a plain object into
 * `[object Object]` and an `Error` into a bare `Name: message` with its cause dropped
 * (ERR-VEZUB arrived as `detail="[object Object]"`). `cmdr/no-stringified-log-values` keeps
 * call sites from doing it anyway.
 *
 * Objects serialize as COMPACT JSON on purpose: an error report tokenizes identity-keyed pairs
 * (`"path":"…"`, `"server":"…"`) inside a `detail="…"` field by matching that shape
 * (`src-tauri/src/redact/detail.rs`), so a path inside a logged object still gets redacted.
 */

import { TypedFailure } from '#lib/ipc/typed-failure.js'

/**
 * Longest rendering of one value, about the 1 KiB the Rust side's `LogDetail` keeps. A report
 * caps a `detail` field far shorter; this bounds the local log against a runaway object.
 */
export const MAX_DESCRIBED_CHARS = 1000

/** How many causes deep the chain goes, against a pathological (non-cyclic) chain. */
const MAX_CAUSE_DEPTH = 8

export function describeForLog(value: unknown): string {
  if (typeof value === 'string') return value
  if (typeof value !== 'object' || value === null) return String(value)
  return cap(value instanceof Error ? describeError(value) : describeObject(value))
}

/** `Name: message`, a `TypedFailure`'s payload, then the cause chain. */
function describeError(error: Error): string {
  const seen = new Set<unknown>()
  const links: string[] = []
  let current: unknown = error
  while (current != null && !seen.has(current) && links.length < MAX_CAUSE_DEPTH) {
    seen.add(current)
    if (!(current instanceof Error)) {
      links.push(describeObjectOrPrimitive(current))
      break
    }
    links.push(errorHeadline(current))
    current = (current as { cause?: unknown }).cause
  }
  return links.join('; caused by ')
}

function errorHeadline(error: Error): string {
  const headline = error.message ? `${error.name}: ${error.message}` : error.name
  return error instanceof TypedFailure ? `${headline} ${toJson(error.failure)}` : headline
}

function describeObjectOrPrimitive(value: unknown): string {
  if (typeof value === 'string') return value
  if (typeof value !== 'object' || value === null) return String(value)
  return describeObject(value)
}

/** A class with its own `toString` (`URL`, `Date`) says it best; anything else goes as JSON. */
function describeObject(value: object): string {
  if (!Array.isArray(value) && hasOwnToString(value)) return String(value)
  return toJson(value)
}

function hasOwnToString(value: object): boolean {
  return (value as { toString?: unknown }).toString !== Object.prototype.toString
}

/** Compact JSON with a cycle guard; nested errors render as text instead of `{}`. */
function toJson(value: unknown): string {
  const ancestors: unknown[] = []
  try {
    return (
      JSON.stringify(value, function (this: unknown, _key, nested: unknown) {
        if (typeof nested === 'bigint') return nested.toString()
        if (typeof nested !== 'object' || nested === null) return nested
        if (nested instanceof Error) return describeError(nested)
        // `this` is the holder of `nested`: unwind the ancestor stack to it, so a value
        // repeated in a sibling isn't mistaken for a cycle.
        while (ancestors.length > 0 && ancestors.at(-1) !== this) ancestors.pop()
        if (ancestors.includes(nested)) return '[Circular]'
        ancestors.push(nested)
        return nested
      }) ?? String(value)
    )
  } catch {
    // A getter that throws, or a Proxy: the value still deserves a line.
    return `[unserializable ${constructorName(value)}]`
  }
}

function constructorName(value: unknown): string {
  const name = (value as { constructor?: { name?: unknown } } | null)?.constructor?.name
  return typeof name === 'string' && name !== '' ? name : 'object'
}

function cap(text: string): string {
  if (text.length <= MAX_DESCRIBED_CHARS) return text
  return `${text.slice(0, MAX_DESCRIBED_CHARS)}…[${String(text.length - MAX_DESCRIBED_CHARS)} chars]`
}
