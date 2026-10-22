/**
 * Tier 3 a11y tests for `MediaFieldPicker.svelte`: open under its button, both groups listed with
 * values and "none" notes and a picked field's ✓, while the read goes on; and filtered to nothing.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MediaFieldPicker from './MediaFieldPicker.svelte'
import { MEDIA_FIELDS } from './media-token'
import type { MediaField } from '#lib/tauri-commands/index.js'
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

const overview = {
  progress: { read: 3, total: 10, done: false },
  fields: (Object.keys(MEDIA_FIELDS) as MediaField[]).map((field) => ({
    field,
    example: field === 'exifTaken' ? '2026-07-14 09.05.30' : field === 'tagArtist' ? 'Daft Punk' : null,
    filesWithIt: field === 'exifTaken' || field === 'tagArtist' ? 3 : 0,
  })),
}

async function mountPicker(): Promise<void> {
  const anchor = document.createElement('button')
  anchor.textContent = 'media'
  document.body.appendChild(anchor)
  const root = document.createElement('div')
  document.body.appendChild(root)
  const picker = mount(MediaFieldPicker, {
    target: root,
    props: {
      anchor,
      open: true,
      progress: overview.progress,
      loadOverview: () => Promise.resolve(overview),
      picked: new Set<MediaField>(['tagArtist']),
      onPick: () => {},
      onClose: () => {},
    },
  })
  cleanup = () => {
    void unmount(picker)
  }
  await settle()
}

describe('MediaFieldPicker a11y', () => {
  it('open, with values, none notes, and a picked field, has no violations', async () => {
    await mountPicker()
    await expectNoA11yViolations(document.body)
  })

  it('filtered to no field has no violations', async () => {
    await mountPicker()
    const filter = document.querySelector<HTMLInputElement>('.ui-popover input')
    if (!filter) throw new Error('no filter box')
    filter.value = 'zzz'
    filter.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    await expectNoA11yViolations(document.body)
  })
})
