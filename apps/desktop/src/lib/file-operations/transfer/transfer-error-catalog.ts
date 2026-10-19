/**
 * The `errors.write.*` catalog lookup the transfer error copy is built from, shared
 * by `transfer-error-messages.ts` and its split-out siblings. A RAW lookup through
 * `getMessage()`, never ICU `t()`: these values carry interpolated paths and HTML
 * escaping that collide with ICU's grammar (`#lib/error-messages/CLAUDE.md`).
 */
import { getMessage } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

/** Substitutes `{token}` placeholders in a catalog value with runtime strings. */
function interpolate(template: string, params: Record<string, string> = {}): string {
  let out = template
  for (const [name, value] of Object.entries(params)) {
    out = out.replaceAll(`{${name}}`, value)
  }
  return out
}

/** Raw catalog lookup for an `errors.write.*` key (no ICU). */
export function w(key: string, params?: Record<string, string>): string {
  const value = getMessage(`errors.write.${key}` as MessageKey)
  return params ? interpolate(value, params) : value
}

export interface FriendlyErrorMessage {
  /** Short title for the error */
  title: string
  /** Main explanation of what happened */
  message: string
  /** Suggestion for what the user can do */
  suggestion: string
}
