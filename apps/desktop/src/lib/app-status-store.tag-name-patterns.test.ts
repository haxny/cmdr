/**
 * The Tags tab's remembered file-name patterns (fork): what a save leaves on disk, and that reading back drops
 * anything that isn't a well-formed pattern. Same fake `@tauri-apps/plugin-store` rig as the write-kind test.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { loadTagNamePatterns, parseTagNamePatterns, saveTagNamePatterns } from './app-status-store'

const disk = vi.hoisted(() => new Map<string, unknown>())

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: (key: string) => Promise.resolve(disk.get(key)),
      set: (key: string, value: unknown) => {
        disk.set(key, value)
        return Promise.resolve()
      },
      save: () => Promise.resolve(),
    }),
  ),
}))

vi.mock('./settings/store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

describe('the tag file-name patterns', () => {
  beforeEach(() => {
    disk.clear()
  })

  it('are empty until the user takes one', async () => {
    expect(await loadTagNamePatterns()).toEqual([])
  })

  it('read back what a save kept', async () => {
    const patterns = [
      { kind: 'format' as const, pattern: '%track% - %title%' },
      { kind: 'regex' as const, pattern: '(?<title>.+)' },
    ]
    await saveTagNamePatterns(patterns)
    expect(await loadTagNamePatterns()).toEqual(patterns)
  })

  it('keep at most ten when saving', async () => {
    const many = Array.from({ length: 13 }, (_, i) => ({ kind: 'format' as const, pattern: `p${String(i)}` }))
    await saveTagNamePatterns(many)
    expect(disk.get('tagNamePatterns')).toHaveLength(10)
  })

  it('are empty when the stored value is not a list', async () => {
    disk.set('tagNamePatterns', 'oops')
    expect(await loadTagNamePatterns()).toEqual([])
  })
})

describe('parseTagNamePatterns', () => {
  it('drops entries that are not a mode with a non-empty pattern, and keeps the rest in order', () => {
    expect(
      parseTagNamePatterns([
        { kind: 'format', pattern: 'ok' },
        { kind: 'glob', pattern: 'bad mode' },
        { kind: 'regex', pattern: '' },
        { kind: 'regex', pattern: 3 },
        null,
        'text',
        { kind: 'regex', pattern: 'fine', extra: 1 },
      ]),
    ).toEqual([
      { kind: 'format', pattern: 'ok' },
      { kind: 'regex', pattern: 'fine' },
    ])
  })

  it('caps a long list at ten', () => {
    const long = Array.from({ length: 15 }, (_, i) => ({ kind: 'format', pattern: `p${String(i)}` }))
    expect(parseTagNamePatterns(long)).toHaveLength(10)
  })
})
