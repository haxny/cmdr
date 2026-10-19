/**
 * Shared by the rules that check calls on a LogTape logger: finding them, and reading the
 * properties argument.
 *
 * Only loggers bound from `getAppLogger(...)` count, so `console.error` (whose second argument
 * really is printed) and unrelated `.error()` methods are never flagged.
 */

const logMethods = new Set(['debug', 'info', 'warn', 'error', 'fatal'])

/**
 * Visitors that hand `onLoggerCall` every `<logger>.<level>(...)` call on a `getAppLogger(...)`
 * binding in the file, on `Program:exit` once every binding is known.
 *
 * @param {(node: import('estree').CallExpression) => void} onLoggerCall
 * @returns {import('eslint').Rule.RuleListener}
 */
export function appLoggerCallVisitors(onLoggerCall) {
  /** Identifiers bound to a `getAppLogger(...)` result in this file. */
  const loggerNames = new Set()
  /** Candidate calls, checked once every binding is known. */
  const pendingCalls = []

  return {
    VariableDeclarator(node) {
      if (
        node.id.type === 'Identifier' &&
        node.init &&
        node.init.type === 'CallExpression' &&
        node.init.callee.type === 'Identifier' &&
        node.init.callee.name === 'getAppLogger'
      ) {
        loggerNames.add(node.id.name)
      }
    },
    CallExpression(node) {
      const callee = node.callee
      if (
        callee.type !== 'MemberExpression' ||
        callee.computed ||
        callee.object.type !== 'Identifier' ||
        callee.property.type !== 'Identifier' ||
        !logMethods.has(callee.property.name)
      ) {
        return
      }
      pendingCalls.push({ node, loggerName: callee.object.name })
    },
    'Program:exit': function () {
      for (const { node, loggerName } of pendingCalls) {
        if (loggerNames.has(loggerName)) onLoggerCall(node)
      }
    },
  }
}

/**
 * The object literal a call passes as its properties argument, unwrapping the
 * lazy `() => ({ ... })` form LogTape also accepts. Returns null when the
 * argument isn't a statically readable object.
 *
 * @param {import('estree').Node | undefined} node
 * @returns {import('estree').ObjectExpression | null}
 */
export function propertiesObject(node) {
  if (!node) return null
  if (node.type === 'ObjectExpression') return node
  if (node.type === 'ArrowFunctionExpression' && node.body.type === 'ObjectExpression') return node.body
  return null
}
