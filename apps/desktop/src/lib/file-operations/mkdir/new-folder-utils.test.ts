import { describe, it, expect } from 'vitest'
import { removeExtension } from './new-folder-utils'

describe('removeExtension', () => {
  it('removes a simple extension', () => {
    expect(removeExtension('photo.jpg')).toBe('photo')
    expect(removeExtension('document.pdf')).toBe('document')
  })

  it('removes a compressed tar extension whole', () => {
    expect(removeExtension('archive.tar.gz')).toBe('archive')
  })

  it('keeps words after a dot, which are part of the name', () => {
    expect(removeExtension('Smlouva v. 2 final')).toBe('Smlouva v. 2 final')
  })

  it('returns filename as-is when no extension', () => {
    expect(removeExtension('Makefile')).toBe('Makefile')
    expect(removeExtension('README')).toBe('README')
  })

  it('returns hidden files as-is (dot at start only)', () => {
    expect(removeExtension('.gitignore')).toBe('.gitignore')
    expect(removeExtension('.env')).toBe('.env')
  })

  it('handles hidden files with extensions', () => {
    expect(removeExtension('.config.json')).toBe('.config')
  })

  it('handles empty string', () => {
    expect(removeExtension('')).toBe('')
  })

  it('handles files with multiple dots', () => {
    expect(removeExtension('my.file.name.txt')).toBe('my.file.name')
  })
})
