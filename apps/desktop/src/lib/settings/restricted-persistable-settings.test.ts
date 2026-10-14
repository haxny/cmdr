/**
 * The viewer's write allowlist has one frontend source, `RESTRICTED_PERSISTABLE_SETTINGS`, and
 * this test pins it to the Rust side: the `setting_id` match in `commands/settings.rs`, which is
 * what the backend forwards to the main window. The variant set itself is type-checked
 * (`satisfies Record<RestrictedWindowPersistableSetting, …>` against the generated bindings), so
 * what can drift unseen is only the id each variant maps to. Parsed from source, not copied, so
 * the test can't drift with them.
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { RESTRICTED_PERSISTABLE_SETTINGS, restrictedPersistableVariant } from './restricted-persistable-settings'

const here = path.dirname(fileURLToPath(import.meta.url))
// here = apps/desktop/src/lib/settings → the desktop app root is three up.
const desktopRoot = path.resolve(here, '../../..')

/** `variant (camelCase, as serde sends it) → setting id`, from the Rust `setting_id` match. */
function rustSettingIds(): Record<string, string> {
  const source = readFileSync(path.join(desktopRoot, 'src-tauri/src/commands/settings.rs'), 'utf8')
  const start = source.indexOf('pub fn setting_id(self)')
  expect(start, 'setting_id not found in commands/settings.rs').toBeGreaterThan(-1)
  const body = source.slice(start, source.indexOf('\n    }\n', start))
  const ids: Record<string, string> = {}
  for (const [, variant, id] of body.matchAll(/Self::(\w+)\s*=>\s*"([^"]+)"/g)) {
    ids[variant.charAt(0).toLowerCase() + variant.slice(1)] = id
  }
  return ids
}

describe('restricted-window persist allowlist', () => {
  it('maps every variant to the same setting id as the Rust enum does', () => {
    const rust = rustSettingIds()
    expect(Object.keys(rust).length, 'parsed no match arms from setting_id').toBeGreaterThan(0)
    expect({ ...RESTRICTED_PERSISTABLE_SETTINGS }).toEqual(rust)
  })

  it('finds a variant by whole setting id only', () => {
    expect(restrictedPersistableVariant('viewer.wordWrap')).toBe('viewerWordWrap')
    expect(restrictedPersistableVariant('viewer.showTextCursor')).toBeUndefined()
    expect(restrictedPersistableVariant('viewer.word')).toBeUndefined()
    expect(restrictedPersistableVariant('constructor')).toBeUndefined()
  })
})
