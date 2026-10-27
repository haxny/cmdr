/** A tag box whose commit throws or rejects: no unhandled rejection, a log line by kind, and a retry sends again. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import EditableCell from './EditableCell.svelte'
import { byId, mountInto, press, typeInto } from './tag-cell.test-helper'

const warn = vi.hoisted(() => vi.fn())
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  vi.clearAllMocks()
})

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('EditableCell', () => {
  it.each([
    ['rejects', () => Promise.reject(new TypeError('the IPC broke'))],
    [
      'throws',
      () => {
        throw new RangeError('the handler broke')
      },
    ],
  ])('a commit that %s is logged by kind, and the same text can be sent again', async (_how, failing) => {
    const onCommit = vi.fn(failing)
    const unhandled = vi.fn()
    process.on('unhandledRejection', unhandled)
    try {
      const mounted = await mountInto(EditableCell, { id: 'cell', value: '', onCommit })
      cleanup = mounted.cleanup
      const box = byId<HTMLInputElement>(mounted.root, 'cell')
      await typeInto(box, 'Dracula')
      await press(box, 'Enter')
      await settle()
      expect(onCommit).toHaveBeenCalledWith('Dracula')
      expect(warn).toHaveBeenCalledTimes(1)
      expect(JSON.stringify(warn.mock.calls[0])).not.toContain('Dracula')
      await press(box, 'Enter')
      await settle()
      expect(onCommit).toHaveBeenCalledTimes(2)
      expect(unhandled).not.toHaveBeenCalled()
    } finally {
      process.off('unhandledRejection', unhandled)
    }
  })
})
