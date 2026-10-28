/**
 * Behavior tests for the media field editor inside `MaskInput`: a `[=…]` token gets a marker, ↓
 * from inside it opens the editor on its format box, a typed format or a preset rewrites the token
 * (a half-typed one waits), the date fallback writes `|modified` and says how many files use it,
 * a number takes digits and a text a range, Enter goes back to the mask without reaching the sheet,
 * and the files around the cursor show what the token gives. The engine is a fake.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'
import type { RangeSampleSource } from './range-sample-source'
import type { FieldSamples } from '#lib/tauri-commands/index.js'

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

/** The tokens the fake engine was asked about, newest last. */
let asked: string[] = []

function fakeSource(): RangeSampleSource {
  return {
    sampleRows: { cursor: 1, rows: [0, 1, 2] },
    rangeSamples: () => Promise.resolve([]),
    fieldSamples: (token): Promise<FieldSamples> => {
      asked.push(token)
      const fallback = token.includes('|modified')
      return Promise.resolve({
        samples: [
          { row: 0, text: '2026-07-14', gap: null },
          {
            row: 1,
            text: fallback ? '2026-01-02' : null,
            gap: fallback ? null : { type: 'noValue', field: 'exifTaken' },
          },
          { row: 2, text: null, gap: { type: 'reading' } },
        ],
        usage: { withValue: 37, fallback: fallback ? 38 : 0, missing: fallback ? 0 : 38, reading: 1 },
      })
    },
  }
}

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  asked = []
  document.body.innerHTML = ''
})

interface Mounted {
  input: HTMLInputElement
  value: () => string
  outerKeys: string[]
}

async function mountField(initial: string): Promise<Mounted> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const outerKeys: string[] = []
  const state = { value: initial }
  const harness = mount(MaskInputFixture, {
    target,
    props: {
      initial,
      onValue: (next: string) => {
        state.value = next
      },
      onOuterKey: (key: string) => outerKeys.push(key),
      source: fakeSource(),
    },
  })
  cleanup = () => {
    void unmount(harness)
  }
  await settle()
  const input = target.querySelector<HTMLInputElement>('input')
  if (!input) throw new Error('no mask field')
  return { input, value: () => state.value, outerKeys }
}

function key(el: Element, k: string): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true })
  el.dispatchEvent(event)
  return event
}

async function open(m: Mounted, caret: number): Promise<void> {
  m.input.focus()
  m.input.setSelectionRange(caret, caret)
  key(m.input, 'ArrowDown')
  await settle()
}

function formatBox(): HTMLInputElement {
  const found = document.querySelector<HTMLInputElement>('.ui-popover .media-editor input:not([type="checkbox"])')
  if (!found) throw new Error('no media editor')
  return found
}

async function type(input: HTMLInputElement, text: string): Promise<void> {
  input.value = text
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await settle()
}

describe('the media field editor', () => {
  it('opens on ↓ inside the token, its format box holding the format', async () => {
    const m = await mountField('[=exif.taken:d] [N]')
    expect(document.querySelector('button[aria-label="Edit field"]')).not.toBeNull()
    await open(m, 5)
    expect(formatBox().value).toBe('d')
    expect(document.activeElement).toBe(formatBox())
    expect(document.querySelector('.media-editor')?.textContent).toContain('Date taken')
  })

  it('writes a typed date format, and waits on one the grammar doesn’t take', async () => {
    const m = await mountField('[=exif.taken]')
    await open(m, 5)
    await type(formatBox(), 'YMD_hms')
    expect(m.value()).toBe('[=exif.taken:YMD_hms]')
    await type(formatBox(), 'YMD_x')
    expect(m.value()).toBe('[=exif.taken:YMD_hms]')
    expect(formatBox().value).toBe('YMD_x')
    await type(formatBox(), '')
    expect(m.value()).toBe('[=exif.taken]')
  })

  it('a preset rewrites the format', async () => {
    const m = await mountField('[=exif.taken]')
    await open(m, 5)
    const date = [...document.querySelectorAll<HTMLButtonElement>('.media-editor button')].find(
      (b) => b.textContent.trim() === 'Date',
    )
    date?.click()
    await settle()
    expect(m.value()).toBe('[=exif.taken:d]')
  })

  it('the fallback writes |modified and says how many files use it', async () => {
    const m = await mountField('[=exif.taken:d]')
    await open(m, 5)
    expect(document.querySelector('.media-editor')?.textContent).toContain('38 files have none and keep their names')
    document.querySelector<HTMLInputElement>('.media-editor input[type="checkbox"]')?.click()
    await settle()
    expect(m.value()).toBe('[=exif.taken|modified:d]')
    expect(asked.at(-1)).toBe('=exif.taken|modified:d')
    expect(document.querySelector('.media-editor')?.textContent).toContain(
      '38 files use the date they were last changed',
    )
  })

  it('shows what the token gives on the files around the cursor, the cursor’s set apart', async () => {
    const m = await mountField('[=exif.taken:d]')
    await open(m, 5)
    const rows = [...document.querySelectorAll('.media-editor li')]
    expect(rows.map((r) => r.textContent.trim())).toEqual(['2026-07-14', 'none', 'reading…'])
    expect(rows[1].getAttribute('aria-current')).toBe('true')
  })

  it('takes digits for a number, a range for a text, and has no fallback for either', async () => {
    const track = await mountField('[=tag.track]')
    await open(track, 5)
    expect(document.querySelector('.media-editor input[type="checkbox"]')).toBeNull()
    await type(formatBox(), '2')
    expect(track.value()).toBe('[=tag.track:2]')
    cleanup?.()
    document.body.innerHTML = ''

    const artist = await mountField('[=tag.artist]')
    await open(artist, 5)
    await type(formatBox(), '1-10')
    expect(artist.value()).toBe('[=tag.artist:1-10]')
  })

  it('says so under the Digits box while it cannot read what was typed, and stops once it can', async () => {
    const track = await mountField('[=tag.track]')
    await open(track, 5)
    expect(document.querySelector('.media-editor .invalid')).toBeNull()
    await type(formatBox(), '0')
    expect(track.value()).toBe('[=tag.track]')
    expect(document.querySelector('.media-editor .invalid')?.textContent).toBe(
      'Type how many digits, like 2, or zeros, like 00.',
    )
    await type(formatBox(), '00')
    expect(document.querySelector('.media-editor .invalid')).toBeNull()
    expect(track.value()).toBe('[=tag.track:2]')
  })

  it('Enter goes back to the mask, and never reaches the sheet', async () => {
    const m = await mountField('[=exif.taken:d]')
    await open(m, 5)
    const enter = key(formatBox(), 'Enter')
    await settle()
    expect(enter.defaultPrevented).toBe(true)
    expect(m.outerKeys).not.toContain('Enter')
    expect(document.querySelector('.media-editor')).toBeNull()
    expect(document.activeElement).toBe(m.input)
  })
})
