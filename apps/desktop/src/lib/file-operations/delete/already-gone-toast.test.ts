import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest'

const { getSettingMock, addToastMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn<(id: string) => boolean>(),
  addToastMock: vi.fn(),
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: getSettingMock,
}))

vi.mock('#lib/ui/toast/index.js', () => ({
  addToast: addToastMock,
}))

vi.mock('./AlreadyGoneToastContent.svelte', () => ({
  default: { __toastContent: 'AlreadyGoneToastContent' },
}))

import { _setLocaleForTests } from '#lib/intl/locale.js'
import { ALREADY_GONE_TOAST_ID, composeAlreadyGoneMessage, maybeShowAlreadyGoneToast } from './already-gone-toast'
import AlreadyGoneToastContent from './AlreadyGoneToastContent.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('composeAlreadyGoneMessage', () => {
  it('names the one item that was already gone', () => {
    expect(composeAlreadyGoneMessage({ itemCount: 1, totalCount: 1, name: 'song.m4a' })).toBe(
      '“song.m4a” was already gone. Another app probably moved or deleted it.',
    )
  })

  it('counts several items that were already gone', () => {
    expect(composeAlreadyGoneMessage({ itemCount: 1234, totalCount: 2000, name: null })).toBe(
      '1,234 items were already gone. Another app probably moved or deleted them.',
    )
  })
})

describe('maybeShowAlreadyGoneToast', () => {
  beforeEach(() => {
    getSettingMock.mockReset()
    addToastMock.mockReset()
    getSettingMock.mockReturnValue(true)
  })

  it('raises a five-second info toast with the composed message', () => {
    maybeShowAlreadyGoneToast({ itemCount: 1, totalCount: 1, name: 'song.m4a' })

    expect(getSettingMock).toHaveBeenCalledWith('fileOperations.showAlreadyGoneToast')
    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0] as [unknown, Record<string, unknown>]
    expect(content).toBe(AlreadyGoneToastContent)
    expect(options).toMatchObject({
      id: ALREADY_GONE_TOAST_ID,
      level: 'info',
      timeoutMs: 5000,
      props: { message: '“song.m4a” was already gone. Another app probably moved or deleted it.' },
    })
  })

  it('stays quiet once the person turned it off', () => {
    getSettingMock.mockReturnValue(false)

    maybeShowAlreadyGoneToast({ itemCount: 1, totalCount: 1, name: 'song.m4a' })

    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('says nothing when nothing was gone', () => {
    maybeShowAlreadyGoneToast(null)
    maybeShowAlreadyGoneToast({ itemCount: 0, totalCount: 3, name: null })

    expect(addToastMock).not.toHaveBeenCalled()
  })
})
