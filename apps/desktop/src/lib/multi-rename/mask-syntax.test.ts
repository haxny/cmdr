import { describe, it, expect } from 'vitest'
import { FOOBAR_INSERTS, foobarUsesMedia, looksLikeFoobar, maskUsesMedia, utf16Offset } from './mask-syntax'
import { DEFAULT_SPEC, specsEqual } from './spec'

/** The user's pattern from the screenshot that showed cut names. */
const READ_BY =
  "[%album artist% - ]'('%year%')' %album% '('cte %performer%, %publisher%')' - %disc%$num(%tracknumber%,2)"

describe('mask syntax', () => {
  it('knows a foobar2000 pattern by a %field% or a $function(', () => {
    expect(looksLikeFoobar(READ_BY)).toBe(true)
    expect(looksLikeFoobar('%album artist%')).toBe(true)
    expect(looksLikeFoobar('$num(1,2)')).toBe(true)
    expect(looksLikeFoobar('$Upper(x)')).toBe(true)
  })

  it('leaves Cmdr masks and plain percent or dollar text alone', () => {
    expect(looksLikeFoobar('[N] [C]')).toBe(false)
    expect(looksLikeFoobar('[=tag.performer] [N]')).toBe(false)
    expect(looksLikeFoobar('100% done')).toBe(false)
    expect(looksLikeFoobar('$5 bill (2)')).toBe(false)
    expect(looksLikeFoobar('50% to 60%')).toBe(false)
  })

  it('says a foobar pattern reads tags only for a tag field outside quotes', () => {
    expect(foobarUsesMedia('%title%')).toBe(true)
    expect(foobarUsesMedia('[%ALBUM ARTIST% - ]%filename%')).toBe(true)
    expect(foobarUsesMedia('%filename% - $upper(%directoryname%)')).toBe(false)
    expect(foobarUsesMedia("'%title%' %filename%")).toBe(false)
    expect(foobarUsesMedia("'it''s' %title%")).toBe(true)
    expect(maskUsesMedia('%title%', 'cmdr')).toBe(false)
    expect(maskUsesMedia('%title%', 'foobar')).toBe(true)
    expect(maskUsesMedia('[=tag.title]', 'cmdr')).toBe(true)
  })

  it('turns the engine byte offset into the field index, Czech letters included', () => {
    expect(utf16Offset('Deník %nope%', 'Deník '.length + 1)).toBe('Deník '.length)
    expect(utf16Offset('abc', 2)).toBe(2)
    expect(utf16Offset('abc', 99)).toBe(3)
    expect(utf16Offset('😀x', 4)).toBe(2)
  })

  it('offers foobar field inserts that are foobar fields', () => {
    expect(FOOBAR_INSERTS.length).toBeGreaterThan(0)
    for (const insert of FOOBAR_INSERTS) expect(insert).toMatch(/^%[a-z ]+%$/)
  })

  it('a spec without a syntax is a Cmdr one, and the syntax counts when comparing', () => {
    expect(DEFAULT_SPEC.maskSyntax).toBe('cmdr')
    const { maskSyntax: _dropped, ...older } = DEFAULT_SPEC
    expect(specsEqual(DEFAULT_SPEC, older)).toBe(true)
    expect(specsEqual(DEFAULT_SPEC, { ...DEFAULT_SPEC, maskSyntax: 'foobar' })).toBe(false)
  })
})
