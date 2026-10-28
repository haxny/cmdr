/** Tier 3 a11y tests for `TagValueEditor.svelte`: a line, and a multiline value flagged invalid. */
import { afterEach, describe, it } from 'vitest'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import TagValueEditor from './TagValueEditor.svelte'
import { mountInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

describe('TagValueEditor a11y', () => {
  it('a single line has no violations', async () => {
    const mounted = await mountInto(TagValueEditor, {
      value: 'Dracula',
      ariaLabel: 'Album for all files',
      onCommit: () => {},
      onClose: () => {},
    })
    cleanup = mounted.cleanup
    await expectNoA11yViolations(mounted.root)
  })

  it('a multiline invalid value has no violations', async () => {
    const mounted = await mountInto(TagValueEditor, {
      value: 'Line one',
      ariaLabel: 'Comment for all files',
      multiline: true,
      invalid: true,
      onCommit: () => {},
      onClose: () => {},
    })
    cleanup = mounted.cleanup
    await expectNoA11yViolations(mounted.root)
  })
})
