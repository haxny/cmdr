import { describe, it, expect } from 'vitest'
import type { OperationItems } from '#lib/ipc/bindings.js'
import { itemsPhrase, itemsTooltip, locationName } from './row-items'

function items(over: Partial<OperationItems> = {}): OperationItems {
  return { count: 3, kinds: null, names: ['a', 'b', 'c'], from: '~/Dropbox', to: 'cmdr-s3-test/target', ...over }
}

describe('itemsPhrase', () => {
  it('names folders first, then files, when the starter said which is which', () => {
    expect(itemsPhrase(items({ kinds: { files: 1, folders: 2 } }))).toBe('2 folders and 1 file')
  })

  it('names one kind alone', () => {
    expect(itemsPhrase(items({ count: 1, kinds: { files: 1, folders: 0 } }))).toBe('1 file')
    expect(itemsPhrase(items({ count: 4, kinds: { files: 0, folders: 4 } }))).toBe('4 folders')
  })

  it('falls back to a plain item count, grouped', () => {
    expect(itemsPhrase(items({ count: 2000 }))).toBe('2,000 items')
    expect(itemsPhrase(items({ count: 1 }))).toBe('1 item')
  })
})

describe('itemsTooltip', () => {
  it('lists the names, one per line', () => {
    expect(itemsTooltip(items())).toBe('a\nb\nc')
  })

  it('says how many more there are past the names it has', () => {
    expect(itemsTooltip(items({ count: 2000, names: ['a', 'b'] }))).toBe('a\nb\n… and 1,998 more')
  })
})

describe('locationName', () => {
  it('is the last folder of a display path', () => {
    expect(locationName('~/Dropbox')).toBe('Dropbox')
    expect(locationName('cmdr-s3-test/target/folder')).toBe('folder')
  })

  it('keeps a path with nothing past its root whole', () => {
    expect(locationName('~')).toBe('~')
    expect(locationName('/')).toBe('/')
    expect(locationName('cmdr-s3-test')).toBe('cmdr-s3-test')
  })
})
