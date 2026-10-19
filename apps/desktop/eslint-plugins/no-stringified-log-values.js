/**
 * ESLint rule: pass a logged value raw, never pre-stringified.
 *
 * Rationale: the frontend log bridge renders every value through `describeForLog`
 * (`src/lib/logging/describe-for-log.ts`), which keeps an `Error`'s cause chain and a
 * `TypedFailure`'s payload and serializes objects as compact JSON the report redactor can
 * tokenize. A call site that stringifies first throws all of that away: `String(obj)` is
 * `[object Object]`, and `e instanceof Error ? e.message : String(e)` drops the error's name,
 * its cause, and any typed payload.
 *
 * Gotcha/Why: ERR-VEZUB (a `DeleteDialog` render crash) reached the report as
 * `detail="[object Object]"`; the cause had to be found by reproducing it.
 *
 * Flags, in a `getAppLogger(...)` logger call:
 *   - `String(x)` as a property value, or as a branch of `??`, `||`, `&&`, or `?:` there
 *     → fixed to `x`;
 *   - `x instanceof Error ? <anything> : String(x)` (or `: x`) → fixed to `x`;
 *   - `${String(x)}` in a template-literal message → fixed to a `{x}` placeholder plus a
 *     properties object (`{error}` for a caught `e` / `err` / `error`). With a non-identifier
 *     argument, a literal brace in the text, or a second argument, it's reported without a fix.
 *
 * `String(x)` nested inside another call (`ids.map((id) => String(id))`) is a real transform and
 * stays alone. Opt out per-line with a reason:
 *   // eslint-disable-next-line cmdr/no-stringified-log-values -- <reason>
 */

import { appLoggerCallVisitors, propertiesObject } from './app-logger-calls.js'

/**
 * The argument of a bare `String(x)` call, or null.
 *
 * @param {import('estree').Node} node
 * @returns {import('estree').Expression | null}
 */
function stringCallArgument(node) {
  if (
    node.type === 'CallExpression' &&
    node.callee.type === 'Identifier' &&
    node.callee.name === 'String' &&
    node.arguments.length === 1 &&
    node.arguments[0].type !== 'SpreadElement'
  ) {
    return node.arguments[0]
  }
  return null
}

/** Node types that read the same with or without parentheses wherever an expression goes. */
const selfDelimiting = new Set([
  'Identifier',
  'MemberExpression',
  'CallExpression',
  'Literal',
  'ThisExpression',
  'TemplateLiteral',
  'ArrayExpression',
  'ObjectExpression',
])

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    fixable: 'code',
    docs: {
      description:
        "Pass a logged value raw: the log bridge's `describeForLog` keeps an Error's cause and a TypedFailure's payload, and `String(x)` renders an object as `[object Object]`.",
      recommended: true,
    },
    messages: {
      stringified:
        "Pass the raw value instead of `String(...)`: the log bridge renders it through `describeForLog`, which keeps an Error's cause chain and a TypedFailure's payload, and serializes objects as JSON (where `String` gives `[object Object]`).",
      errorTernary:
        "Pass the caught value raw instead of this `instanceof Error ? … : String(…)` ternary: the log bridge's `describeForLog` already renders an Error as `Name: message` plus its cause chain, and anything else readably.",
    },
    schema: [],
  },
  create(context) {
    const sourceCode = context.sourceCode

    /** Source of `node`, parenthesized when splicing it into an expression could regroup it. */
    function spliceText(node) {
      const text = sourceCode.getText(node)
      return selfDelimiting.has(node.type) ? text : `(${text})`
    }

    /**
     * The raw value an `x instanceof Error ? … : String(x)` ternary stringifies, or null.
     *
     * @param {import('estree').Node} node
     */
    function errorTernarySubject(node) {
      if (node.type !== 'ConditionalExpression') return null
      const test = node.test
      if (
        test.type !== 'BinaryExpression' ||
        test.operator !== 'instanceof' ||
        test.right.type !== 'Identifier' ||
        test.right.name !== 'Error'
      ) {
        return null
      }
      const subject = sourceCode.getText(test.left)
      const fallback = stringCallArgument(node.alternate) ?? node.alternate
      return sourceCode.getText(fallback) === subject ? test.left : null
    }

    /**
     * Report and fix a value position: a property value, or a branch of a logical or
     * conditional expression in one.
     *
     * @param {import('estree').Node} node
     * @param {import('estree').Property | null} property the property whose whole value `node` is
     */
    function checkValue(node, property) {
      const subject = errorTernarySubject(node)
      const raw = subject ?? stringCallArgument(node)
      if (raw) {
        context.report({
          node,
          messageId: subject ? 'errorTernary' : 'stringified',
          fix(fixer) {
            const rawText = sourceCode.getText(raw)
            // `{ error: String(error) }` reads best as `{ error }`.
            if (property && property.key.type === 'Identifier' && property.key.name === rawText) {
              return fixer.replaceText(property, rawText)
            }
            return fixer.replaceText(node, spliceText(raw))
          },
        })
        return
      }
      if (node.type === 'LogicalExpression') {
        checkValue(node.left, null)
        checkValue(node.right, null)
      } else if (node.type === 'ConditionalExpression') {
        checkValue(node.consequent, null)
        checkValue(node.alternate, null)
      }
    }

    /**
     * `${String(x)}` in a template-literal message: rewrite each into a `{x}` placeholder plus a
     * properties object, so the bridge gets `x` raw. A caught value (`e`, `err`, `error`) becomes
     * `{error}`, which the bridge renders as a `detail` field a report redacts and caps.
     */
    function checkTemplateMessage(call, template) {
      const stringified = template.expressions.filter((expression) => stringCallArgument(expression) !== null)
      if (stringified.length === 0) return
      const fixText = templateFixText(call, template, stringified)
      for (const expression of stringified) {
        context.report({
          node: expression,
          messageId: 'stringified',
          // One fix per call: the first report rewrites the whole argument list.
          fix:
            fixText !== null && expression === stringified[0]
              ? (fixer) => fixer.replaceTextRange([template.range[0], call.range[1] - 1], fixText)
              : null,
        })
      }
    }

    /** The rewritten argument list, or null when there's no safe mechanical rewrite. */
    function templateFixText(call, template, stringified) {
      if (call.arguments.length !== 1) return null
      // Literal braces in the text would read as placeholders once LogTape parses the message.
      if (template.quasis.some((quasi) => /[{}]/.test(quasi.value.raw))) return null

      /** placeholder name → source text of its value */
      const valueByName = new Map()
      const nameByExpression = new Map()
      for (const expression of stringified) {
        const raw = stringCallArgument(expression)
        if (raw.type !== 'Identifier') return null
        const name = ['e', 'err', 'error'].includes(raw.name) ? 'error' : raw.name
        if (valueByName.has(name) && valueByName.get(name) !== raw.name) return null
        valueByName.set(name, raw.name)
        nameByExpression.set(expression, name)
      }

      let body = ''
      template.quasis.forEach((quasi, index) => {
        body += quasi.value.raw
        const expression = template.expressions[index]
        if (!expression) return
        const name = nameByExpression.get(expression)
        body += name === undefined ? `\${${sourceCode.getText(expression)}}` : `{${name}}`
      })

      const properties = [...valueByName].map(([name, value]) => (name === value ? name : `${name}: ${value}`))
      return `${quoteMessage(body, nameByExpression.size === template.expressions.length)}, { ${properties.join(', ')} }`
    }

    /** A plain string literal when nothing is left to interpolate, else the template literal. */
    function quoteMessage(body, noInterpolationLeft) {
      if (noInterpolationLeft && !body.includes('\\')) {
        if (!body.includes("'")) return `'${body}'`
        if (!body.includes('"')) return `"${body}"`
      }
      return `\`${body}\``
    }

    return appLoggerCallVisitors((call) => {
      const message = call.arguments[0]
      if (message && message.type === 'TemplateLiteral') checkTemplateMessage(call, message)

      const properties = propertiesObject(call.arguments[1])
      if (!properties) return
      for (const property of properties.properties) {
        if (property.type !== 'Property') continue
        checkValue(property.value, property.shorthand ? null : property)
      }
    })
  },
}
