/**
 * ESLint rule: every property passed to a LogTape logger must appear as a
 * placeholder in that call's message template.
 *
 * Rationale: the frontend log bridge (`src/lib/logging/log-bridge.ts`) forwards
 * ONLY the rendered message string to Rust. `FrontendLogEntry` carries
 * `level`, `category`, and `message`, and nothing reads `record.properties`, so
 * a field the template never names is discarded at the IPC boundary: it reaches
 * neither the log file nor an error-report bundle. The loss is silent, and it
 * bites exactly when it costs the most.
 *
 * Gotcha/Why: ERR-BPAPM (2026-09-08) was a trash refused by
 * `NSFileManager.trashItemAtURL`. The frontend logged
 * `log.error('{op} error: {errorType}', { op, errorType, error })`, so the
 * bundle recorded `Trash error: io_error` and the `error` payload holding the
 * OS's actual reason never left the renderer. The reason is unrecoverable.
 *
 * Applies at EVERY level, not just error/warn: the bridge drops properties the
 * same way for `debug` and `info`, so an unrendered `{ id }` there is the same
 * defect with a quieter failure mode.
 *
 * Placeholder parsing mirrors LogTape 2.3.0's `parseMessageTemplate`:
 *   - `{{` escapes a literal brace and starts no placeholder,
 *   - a `{` with no closing `}` is literal text,
 *   - the key is trimmed, so `{ count }` renders `count`,
 *   - `{*}` renders the whole properties bag, so nothing can be dropped,
 *   - nested access (`{error.message}`, `{error?.cause}`, `{items[0]}`)
 *     renders its ROOT field, so `error` and `items` count as rendered.
 *
 * Only loggers bound from `getAppLogger(...)` are checked, so `console.error`
 * (whose second argument really is printed) and unrelated `.error()` methods
 * are never flagged.
 *
 * Opt out per-line if a field is deliberately carried for a non-bridge sink:
 *   // eslint-disable-next-line cmdr/no-unrendered-log-fields -- <reason>
 */

import { appLoggerCallVisitors, propertiesObject } from './app-logger-calls.js'

/**
 * Extract the placeholder keys LogTape would resolve from a message template,
 * following the same scan as `parseMessageTemplate`.
 *
 * @param {string} template
 * @returns {{ keys: string[], rendersEverything: boolean }}
 */
function parsePlaceholders(template) {
  const keys = []
  let rendersEverything = false
  for (let i = 0; i < template.length; i++) {
    const char = template[i]
    if (char === '{') {
      // `{{` is an escaped literal brace, not the start of a placeholder.
      if (template[i + 1] === '{') {
        i++
        continue
      }
      const closeIndex = template.indexOf('}', i + 1)
      // An unterminated `{` is literal text.
      if (closeIndex === -1) continue
      const key = template.slice(i + 1, closeIndex).trim()
      if (key === '*') rendersEverything = true
      else keys.push(key)
      i = closeIndex
    } else if (char === '}' && template[i + 1] === '}') {
      i++
    }
  }
  return { keys, rendersEverything }
}

/**
 * The field a placeholder resolves against: the segment before the first
 * nested-access operator (`.`, `[`, `?`).
 *
 * @param {string} key
 * @returns {string}
 */
function rootField(key) {
  const cut = key.search(/[.[?]/)
  return cut === -1 ? key : key.slice(0, cut)
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Every property passed to a logger must appear as a placeholder in the message: the frontend log bridge forwards only the rendered string, so unrendered fields are silently discarded.',
      recommended: true,
    },
    messages: {
      unrenderedField:
        '`{{ field }}` is passed to the logger but never rendered by the message, so the log bridge discards it: it reaches neither the log file nor an error-report bundle. Add `{{{ field }}}` to the template (or `{{{ field }}.someProp}` for one field of it), or drop the property.',
    },
    schema: [],
  },
  create(context) {
    return appLoggerCallVisitors((node) => {
      const template = node.arguments[0]
      // A computed template can't be analyzed; say nothing rather than guess.
      if (!template || template.type !== 'Literal' || typeof template.value !== 'string') return

      const properties = propertiesObject(node.arguments[1])
      if (!properties) return

      // A spread makes the key set unknowable, so checking the rest would
      // report fields the spread might legitimately be renaming around.
      if (properties.properties.some((property) => property.type === 'SpreadElement')) return

      const { keys, rendersEverything } = parsePlaceholders(template.value)
      if (rendersEverything) return
      const rendered = new Set(keys.map(rootField))

      for (const property of properties.properties) {
        if (property.type !== 'Property' || property.computed) continue
        const key =
          property.key.type === 'Identifier'
            ? property.key.name
            : property.key.type === 'Literal' && typeof property.key.value === 'string'
              ? property.key.value
              : null
        if (key === null || rendered.has(key)) continue
        context.report({ node: property, messageId: 'unrenderedField', data: { field: key } })
      }
    })
  },
}
