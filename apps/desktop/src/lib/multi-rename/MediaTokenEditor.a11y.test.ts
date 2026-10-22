/**
 * Tier 3 a11y tests for `MediaTokenEditor.svelte`: on its own, with no files to show (a field outside
 * the sheet), and open under a media token in `MaskInput`, for a date (presets, the fallback, and the
 * files around the cursor, some still reading) and a text field.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'
import MediaTokenEditor from './MediaTokenEditor.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

describe('MediaTokenEditor a11y', () => {
  it('the editor with no files to show has no violations', async () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const editor = mount(MediaTokenEditor, {
      target: root,
      props: {
        value: { field: 'exifTaken', fallback: false, format: { kind: 'date', text: 'd' } },
        onChange: () => {},
        onDone: () => {},
      },
    })
    cleanup = () => {
      void unmount(editor)
    }
    await settle()
    await expectNoA11yViolations(root)
  })
})

describe('the media editor open in a mask field a11y', () => {
  async function mountMedia(initial: string): Promise<void> {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const source = {
      sampleRows: { cursor: 0, rows: [0, 1, 2] },
      rangeSamples: () => Promise.resolve([]),
      fieldSamples: () =>
        Promise.resolve({
          samples: [
            { row: 0, text: '2026-07-14', gap: null },
            { row: 1, text: null, gap: { type: 'noValue' as const, field: 'exifTaken' as const } },
            { row: 2, text: null, gap: { type: 'reading' as const } },
          ],
          usage: { withValue: 1, fallback: 1, missing: 1, reading: 1 },
        }),
    }
    const fixture = mount(MaskInputFixture, {
      target: root,
      props: { initial, onValue: () => {}, onOuterKey: () => {}, source },
    })
    cleanup = () => {
      void unmount(fixture)
    }
    await settle()
    const input = root.querySelector('input')
    input?.focus()
    input?.setSelectionRange(4, 4)
    input?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }))
    await settle()
    if (!document.querySelector('.ui-popover .media-editor li')) {
      throw new Error('the media editor did not open')
    }
  }

  it('a date field with its fallback on has no violations', async () => {
    await mountMedia('[=exif.taken|modified:d]')
    await expectNoA11yViolations(document.body)
  })

  it('a text field has no violations', async () => {
    await mountMedia('[=tag.artist:1-10]')
    await expectNoA11yViolations(document.body)
  })
})
