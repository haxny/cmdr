/**
 * TDD for the message-key codegen (`gen-message-keys-lib.ts`): catalog → the
 * `MessageKey` union, plus the missing-key report (a build failure).
 *
 * The codegen reads `messages/en/*.json` and scans source for `t()`/`<Trans>`/
 * `getMessage()` key usages. These tests drive the pure pieces (the catalog
 * parse, the source scan, and the missing-key diff) against in-memory inputs,
 * so no real catalog or source tree is touched.
 */
import { describe, it, expect } from 'vitest'
import {
  collectCatalogKeys,
  stripMetadataKeys,
  extractUsedKeys,
  findMissingKeys,
  emitKeysModule,
} from './gen-message-keys-lib.ts'

describe('stripMetadataKeys', () => {
  it('keeps message keys and drops ARB-style @key metadata entries', () => {
    const raw = {
      'transfer.trash': 'Moved {countText} to trash',
      '@transfer.trash': { description: 'meta', screenshot: 'x.png' },
      'transfer.delete': 'Deleted',
    }
    expect(stripMetadataKeys(raw)).toEqual(['transfer.trash', 'transfer.delete'])
  })
})

describe('collectCatalogKeys', () => {
  it('merges keys across files, sorts, and dedupes', () => {
    const files = {
      'common.json': { 'common.cancel': 'Cancel', '@common.cancel': { description: 'm' } },
      'transfer.json': { 'transfer.trash': 'x', 'transfer.delete': 'y' },
    }
    expect(collectCatalogKeys(files)).toEqual(['common.cancel', 'transfer.delete', 'transfer.trash'])
  })
})

describe('extractUsedKeys', () => {
  it("finds t('key'), tString('key'), getMessage('key'), and <Trans key=\"...\">", () => {
    const src = [
      `const a = t('transfer.trash', { count: 1 })`,
      `const b = tString("transfer.delete")`,
      `const c = getMessage('common.cancel')`,
      `<Trans key="common.downloadsFdaHint" />`,
      "<Trans key='settings.title' params={p} />",
    ].join('\n')
    expect(extractUsedKeys(src)).toEqual(
      new Set(['transfer.trash', 'transfer.delete', 'common.cancel', 'common.downloadsFdaHint', 'settings.title']),
    )
  })

  it('finds keys passed indirectly via a `*Key`/`*Keys` property (the settings registry pattern)', () => {
    // The settings registry stores message KEYS in `labelKey`/`descriptionKey`
    // fields and resolves them through `t(variable)` (not a literal), so the
    // scan must recognize the key at its literal definition site.
    const src = [
      `labelKey: 'settings.theme.mode.label',`,
      `descriptionKey: "settings.theme.mode.description",`,
      `{ value: 'light', labelKey: 'settings.theme.mode.opt.light' },`,
    ].join('\n')
    expect(extractUsedKeys(src)).toEqual(
      new Set(['settings.theme.mode.label', 'settings.theme.mode.description', 'settings.theme.mode.opt.light']),
    )
  })

  it('ignores non-message calls and dynamic (non-literal) keys', () => {
    const src = [
      `translate(key)`, // dynamic
      `t(dynamicKey)`, // dynamic, not extractable
      `something('not.a.key')`, // not a message accessor
      `t(\`transfer.\${x}\`)`, // template with expression, skipped
    ].join('\n')
    expect(extractUsedKeys(src)).toEqual(new Set())
  })
})

describe('findMissingKeys', () => {
  it('reports keys used in code but missing from the catalog', () => {
    expect(findMissingKeys(['transfer.trash'], new Set(['transfer.trash', 'transfer.missing']))).toEqual([
      'transfer.missing',
    ])
  })

  it('ignores catalog keys nobody uses (the `message-keys-unused` Go check owns those)', () => {
    expect(findMissingKeys(['transfer.trash', 'transfer.unused'], new Set(['transfer.trash']))).toEqual([])
  })

  it('returns the missing keys sorted', () => {
    expect(findMissingKeys(['a.one'], new Set(['z.two', 'a.one', 'b.three']))).toEqual(['b.three', 'z.two'])
  })
})

describe('emitKeysModule', () => {
  it('emits a string-literal union (in the order given) with the do-not-edit header', () => {
    // The caller (collectCatalogKeys) sorts; emit reflects the given order.
    const out = emitKeysModule(['common.cancel', 'transfer.trash'])
    expect(out).toContain('AUTO-GENERATED')
    expect(out).toContain('Do not edit by hand')
    expect(out).toContain('export type MessageKey =')
    expect(out).toContain("  | 'common.cancel'")
    expect(out).toContain("  | 'transfer.trash'")
    // Sorted: common before transfer.
    expect(out.indexOf("'common.cancel'")).toBeLessThan(out.indexOf("'transfer.trash'"))
  })

  it('emits `never` for an empty catalog', () => {
    expect(emitKeysModule([])).toContain('  never')
  })
})
