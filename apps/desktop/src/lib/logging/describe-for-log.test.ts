/**
 * `describeForLog` is the one place a logged value becomes text, so a caught value of any shape
 * reaches the log readable: never `[object Object]`, never an `Error` stripped of its cause or a
 * `TypedFailure` stripped of its payload (ERR-VEZUB arrived as `detail="[object Object]"`).
 */
import { describe, it, expect } from 'vitest'
import { TypedFailure } from '#lib/ipc/typed-failure.js'
import { describeForLog, MAX_DESCRIBED_CHARS } from './describe-for-log'

class EjectFailureForTest extends TypedFailure<{ type: string; path: string }> {
  constructor(failure: { type: string; path: string }) {
    super(failure, `eject refused: ${failure.type}`)
    this.name = 'EjectFailureForTest'
  }
}

describe('describeForLog', () => {
  it('passes strings through verbatim', () => {
    expect(describeForLog('Loaded ')).toBe('Loaded ')
    expect(describeForLog('{"already":"json"}')).toBe('{"already":"json"}')
  })

  it('renders primitives the way String does', () => {
    expect(describeForLog(42)).toBe('42')
    expect(describeForLog(true)).toBe('true')
    expect(describeForLog(null)).toBe('null')
    expect(describeForLog(undefined)).toBe('undefined')
    expect(describeForLog(10n)).toBe('10')
  })

  it('renders an Error as name and message', () => {
    expect(describeForLog(new TypeError('x is not a function'))).toBe('TypeError: x is not a function')
  })

  it('follows the cause chain', () => {
    const root = new Error('EACCES')
    const middle = new Error('read failed', { cause: root })
    const outer = new Error('load failed', { cause: middle })
    expect(describeForLog(outer)).toBe('Error: load failed; caused by Error: read failed; caused by Error: EACCES')
  })

  it('renders a non-Error cause too', () => {
    expect(describeForLog(new Error('load failed', { cause: { code: 13 } }))).toBe(
      'Error: load failed; caused by {"code":13}',
    )
  })

  it('stops on a cyclic cause chain', () => {
    const a = new Error('a')
    const b = new Error('b', { cause: a })
    ;(a as Error & { cause: unknown }).cause = b
    expect(describeForLog(a)).toBe('Error: a; caused by Error: b')
  })

  it("adds a TypedFailure's payload as compact JSON, which the report redactor tokenizes by key", () => {
    const failure = new EjectFailureForTest({ type: 'busy', path: '/Volumes/Client Share' })
    expect(describeForLog(failure)).toBe(
      'EjectFailureForTest: eject refused: busy {"type":"busy","path":"/Volumes/Client Share"}',
    )
  })

  it('renders plain objects and arrays as compact JSON', () => {
    expect(describeForLog({ type: 'notFound', server: 'NAS' })).toBe('{"type":"notFound","server":"NAS"}')
    expect(describeForLog([1, 'two'])).toBe('[1,"two"]')
  })

  it('renders an Error nested in an object instead of dropping it to {}', () => {
    expect(describeForLog({ op: 'copy', error: new RangeError('too big') })).toBe(
      '{"op":"copy","error":"RangeError: too big"}',
    )
  })

  it('guards against cycles', () => {
    const node: Record<string, unknown> = { name: 'a' }
    node.self = node
    expect(describeForLog(node)).toBe('{"name":"a","self":"[Circular]"}')
  })

  it('keeps a repeated (non-cyclic) reference', () => {
    const shared = { id: 1 }
    expect(describeForLog({ a: shared, b: shared })).toBe('{"a":{"id":1},"b":{"id":1}}')
  })

  it('caps a huge object and says how much it cut', () => {
    const described = describeForLog({ blob: 'x'.repeat(MAX_DESCRIBED_CHARS * 2) })
    expect(described.length).toBeLessThan(MAX_DESCRIBED_CHARS + 30)
    expect(described).toMatch(/…\[\d+ chars]$/)
  })

  it("uses a class's own toString when it has one", () => {
    expect(describeForLog(new URL('https://example.com/a'))).toBe('https://example.com/a')
  })

  it('serializes a class instance without its own toString as JSON', () => {
    class Point {
      constructor(
        readonly x: number,
        readonly y: number,
      ) {}
    }
    expect(describeForLog(new Point(1, 2))).toBe('{"x":1,"y":2}')
  })

  it("still returns a line for a value that won't serialize", () => {
    const hostile = {
      get boom(): never {
        throw new Error('getter threw')
      },
    }
    expect(describeForLog(hostile)).toBe('[unserializable Object]')
  })
})
