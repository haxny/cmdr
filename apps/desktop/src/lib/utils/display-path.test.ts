import { describe, expect, it } from 'vitest'
import { displayPath } from './display-path'

/** A twin's row path, the way the MTP backend lists it (`cmdr_fs::twin_path`). */
function twinRowPath(folder: string, handle: number, name: string): string {
  return `${folder}/\u0000${String(handle)}/${name}`
}

describe('displayPath', () => {
  it('takes a twin’s pick out of its row path', () => {
    expect(displayPath(twinRowPath('/DCIM', 4242, 'IMG_1.JPG'))).toBe('/DCIM/IMG_1.JPG')
    expect(displayPath(`${twinRowPath('/DCIM', 7, 'Trip')}/a.jpg`)).toBe('/DCIM/Trip/a.jpg')
    expect(displayPath('/DCIM/\u00009')).toBe('/DCIM')
    expect(displayPath('\u00003/IMG_1.JPG')).toBe('IMG_1.JPG')
  })

  it('reads text holding twin paths with the plain paths', () => {
    const text = `“${twinRowPath('/DCIM', 1, 'a.jpg')}” and “${twinRowPath('/DCIM', 2, 'a.jpg')}”`
    expect(displayPath(text)).toBe('“/DCIM/a.jpg” and “/DCIM/a.jpg”')
  })

  it('leaves a NUL that is no pick alone', () => {
    expect(displayPath('modified \u0000')).toBe('modified \u0000')
    expect(displayPath('/x/\u000012ab/y')).toBe('/x/\u000012ab/y')
  })

  it('leaves an ordinary path alone', () => {
    expect(displayPath('/Users/me/notes.txt')).toBe('/Users/me/notes.txt')
  })
})
