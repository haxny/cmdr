import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './no-stringified-log-values.js'

const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
})

const svelteRuleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

const L = `const log = getAppLogger('x')\n`

ruleTester.run('no-stringified-log-values', rule, {
  valid: [
    // The raw value: the bridge renders it through `describeForLog`.
    `${L}log.error('Trash refused it: {error}', { error: e })`,
    `${L}log.error('Trash refused it: {error}', { error })`,
    // Reading one field is a choice, not a lossy stringification.
    `${L}log.error('Boom: {reason}', { reason: e.message })`,
    // `String` deep inside another call is a real transform, not a log rendering.
    `${L}log.info('Ids: {ids}', { ids: ids.map((id) => String(id)).join(',') })`,
    // A template literal with no `String(...)` in it.
    `${L}log.info(\`Offering it after \${days} days\`)`,
    // Not an app logger.
    `console.error('Boom', String(e))`,
    `const log = makeSomethingElse('x')\nlog.error('Boom {a}', { a: String(e) })`,
    // A ternary that isn't the `instanceof Error` shape stays alone.
    `${L}log.warn('x {mode}', { mode: fast ? 'fast' : 'slow' })`,
    // Non-log uses of the ternary are out of scope.
    `const message = e instanceof Error ? e.message : String(e)`,
  ],
  invalid: [
    {
      code: `${L}log.error('Trash refused it: {error}', { error: String(e) })`,
      output: `${L}log.error('Trash refused it: {error}', { error: e })`,
      errors: [{ messageId: 'stringified' }],
    },
    {
      code: `${L}log.warn('Failed: {error}', () => ({ error: String(err) }))`,
      output: `${L}log.warn('Failed: {error}', () => ({ error: err }))`,
      errors: [{ messageId: 'stringified' }],
    },
    // Every level.
    {
      code: `${L}log.debug('n={count}', { count: String(count) })`,
      output: `${L}log.debug('n={count}', { count })`,
      errors: [{ messageId: 'stringified' }],
    },
    // The ternary collapses to the raw value, whatever the Error branch read.
    {
      code: `${L}log.error('Boom: {error}', { error: e instanceof Error ? e.message : String(e) })`,
      output: `${L}log.error('Boom: {error}', { error: e })`,
      errors: [{ messageId: 'errorTernary' }],
    },
    {
      code: `${L}log.error('Boom: {error}', { error: error instanceof Error ? \`\${error.name}: \${error.message}\` : String(error) })`,
      output: `${L}log.error('Boom: {error}', { error })`,
      errors: [{ messageId: 'errorTernary' }],
    },
    {
      code: `${L}log.error('Boom: {error}', { error: e instanceof Error ? e.message : e })`,
      output: `${L}log.error('Boom: {error}', { error: e })`,
      errors: [{ messageId: 'errorTernary' }],
    },
    // A fallback branch of `??` or `||` is a value position too.
    {
      code: `${L}log.warn('x {reason}', { reason: kind ?? String(e) })`,
      output: `${L}log.warn('x {reason}', { reason: kind ?? e })`,
      errors: [{ messageId: 'stringified' }],
    },
    {
      code: `${L}log.warn('x {reason}', { reason: (e as { type?: string }).type ?? String(e) })`,
      output: `${L}log.warn('x {reason}', { reason: (e as { type?: string }).type ?? e })`,
      errors: [{ messageId: 'stringified' }],
      languageOptions: { parser: (await import('typescript-eslint')).parser },
    },
    // A complex argument keeps its grouping.
    {
      code: `${L}log.warn('x {a}', { a: String(b || c) })`,
      output: `${L}log.warn('x {a}', { a: (b || c) })`,
      errors: [{ messageId: 'stringified' }],
    },
    // A template literal message gets a placeholder and a properties object instead. A caught
    // value becomes `{error}`, which the bridge renders as a `detail` field a report redacts.
    {
      code: `${L}log.warn(\`Couldn't ask about the Dock: \${String(e)}\`)`,
      output: `${L}log.warn("Couldn't ask about the Dock: {error}", { error: e })`,
      errors: [{ messageId: 'stringified' }],
    },
    {
      code: `${L}log.warn(\`Dock pin failed: \${String(err)}\`)`,
      output: `${L}log.warn('Dock pin failed: {error}', { error: err })`,
      errors: [{ messageId: 'stringified' }],
    },
    // Other interpolations stay; every `String(x)` becomes its own placeholder.
    {
      code: `${L}log.info(\`\${label(op)} took \${String(ms)} ms, \${String(error)}\`)`,
      output: `${L}log.info(\`\${label(op)} took {ms} ms, {error}\`, { ms, error })`,
      errors: [{ messageId: 'stringified' }, { messageId: 'stringified' }],
    },
    // A non-identifier argument has no name to give its placeholder; report without a fix.
    {
      code: `${L}log.info(\`after \${String(a.b)}\`)`,
      output: null,
      errors: [{ messageId: 'stringified' }],
    },
    // A literal brace in the text would read as a placeholder; report without a fix.
    {
      code: `${L}log.info(\`{x} after \${String(n)}\`)`,
      output: null,
      errors: [{ messageId: 'stringified' }],
    },
    // A second argument already exists; report without a fix.
    {
      code: `${L}log.info(\`after \${String(n)}\`, extra)`,
      output: null,
      errors: [{ messageId: 'stringified' }],
    },
  ],
})

svelteRuleTester.run('no-stringified-log-values (svelte)', rule, {
  valid: [],
  invalid: [
    {
      code: `<script lang="ts">
               const log = getAppLogger('x')
               function go() { log.error('Boom: {error}', { error: String(e) }) }
             </script>`,
      output: `<script lang="ts">
               const log = getAppLogger('x')
               function go() { log.error('Boom: {error}', { error: e }) }
             </script>`,
      filename: 'src/lib/whatever/Thing.svelte',
      errors: [{ messageId: 'stringified' }],
    },
  ],
})
