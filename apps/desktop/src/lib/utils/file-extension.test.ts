/**
 * The frontend mirror of the product's one extension rule
 * (`crates/cmdr-fs/src/file_extension.rs`). Both suites run the cases in
 * `file_extension_cases.json`, so the two languages can't drift apart.
 */

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  COMPOUND_EXTENSIONS,
  PACKAGE_EXTENSIONS,
  fileExtension,
  nameWithoutExtension,
  withExtension,
} from './file-extension'

interface Spec {
  packageExtensions: string[]
  compoundExtensions: string[]
  cases: { name: string; isDirectory: boolean; extension: string }[]
}

const spec = JSON.parse(
  readFileSync(resolve(process.cwd(), '../../crates/cmdr-fs/src/file_extension_cases.json'), 'utf8'),
) as Spec

describe('fileExtension ↔ the backend`s `extension_of`', () => {
  it.each(spec.cases)('$name (folder: $isDirectory) → "$extension"', ({ name, isDirectory, extension }) => {
    expect(fileExtension(name, isDirectory)).toBe(extension)
  })

  it('knows exactly the package extensions the shared spec does', () => {
    expect([...PACKAGE_EXTENSIONS].sort()).toEqual([...spec.packageExtensions].sort())
  })

  it('knows exactly the compound extensions the shared spec does', () => {
    expect([...COMPOUND_EXTENSIONS]).toEqual(spec.compoundExtensions)
  })
})

describe('withExtension', () => {
  it('swaps, adds, and removes an extension', () => {
    expect(withExtension('photo.png', 'jpg', false)).toBe('photo.jpg')
    expect(withExtension('photo', 'jpg', false)).toBe('photo.jpg')
    expect(withExtension('README.md', '', false)).toBe('README')
    expect(withExtension('src.tar.gz', 'zip', false)).toBe('src.zip')
    expect(withExtension('Smlouva v. 2 final', 'pdf', false)).toBe('Smlouva v. 2 final.pdf')
  })
})

describe('nameWithoutExtension', () => {
  it('strips the extension and its dot, and nothing when there is none', () => {
    expect(nameWithoutExtension('report.pdf', false)).toBe('report')
    expect(nameWithoutExtension('src.tar.gz', false)).toBe('src')
    expect(nameWithoutExtension('Smlouva v. 2 final', false)).toBe('Smlouva v. 2 final')
    expect(nameWithoutExtension('release.v1.2', true)).toBe('release.v1.2')
    expect(nameWithoutExtension('Cmdr.app', true)).toBe('Cmdr')
    expect(nameWithoutExtension('.gitignore', false)).toBe('.gitignore')
  })
})
