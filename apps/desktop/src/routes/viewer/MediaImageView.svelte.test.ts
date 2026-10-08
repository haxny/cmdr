/**
 * The image view's keys, which the viewer page routes through
 * `MediaImageView.handleKey` so they work without clicking the image first:
 * which keys it consumes, which it leaves to the page (the `1`–`3` mode keys,
 * anything with a modifier), and that + / - really zoom and 0 fits again.
 */

import { describe, it, expect } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import MediaImageView from './MediaImageView.svelte'

type ImageView = { handleKey: (e: KeyboardEvent) => boolean }

function mountView(): { view: ImageView; img: () => HTMLImageElement; destroy: () => void } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const view = mount(MediaImageView, { target, props: { src: 'asset://x.png', fileName: 'x.png' } }) as ImageView
  return {
    view,
    img: () => target.querySelector('img') as HTMLImageElement,
    destroy: () => {
      void unmount(view as never)
      target.remove()
    },
  }
}

const key = (k: string, mods: KeyboardEventInit = {}) =>
  new KeyboardEvent('keydown', { key: k, cancelable: true, ...mods })

describe('MediaImageView.handleKey', () => {
  it('consumes the zoom and fit keys, and + / - change the zoom', () => {
    const { view, img, destroy } = mountView()
    expect(img().style.transform).toBe('none')

    const plus = key('+')
    expect(view.handleKey(plus)).toBe(true)
    expect(plus.defaultPrevented).toBe(true)
    flushSync()
    expect(img().style.transform).toContain('scale(1.25)')

    expect(view.handleKey(key('-'))).toBe(true)
    flushSync()
    expect(img().style.transform).toContain('scale(1)')

    expect(view.handleKey(key('0'))).toBe(true)
    flushSync()
    expect(img().style.transform).toBe('none')

    for (const k of ['=', 'Enter', ' ']) expect(view.handleKey(key(k))).toBe(true)
    destroy()
  })

  it('leaves the mode keys and modifier combos to the viewer page', () => {
    const { view, destroy } = mountView()
    for (const k of ['1', '2', '3', 'a', 'Escape']) {
      const e = key(k)
      expect(view.handleKey(e)).toBe(false)
      expect(e.defaultPrevented).toBe(false)
    }
    expect(view.handleKey(key('+', { metaKey: true }))).toBe(false)
    expect(view.handleKey(key('-', { ctrlKey: true }))).toBe(false)
    expect(view.handleKey(key('0', { altKey: true }))).toBe(false)
    destroy()
  })
})
