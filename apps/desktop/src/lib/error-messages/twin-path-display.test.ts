/**
 * A same-named twin's row path carries a NUL pick (`#lib/utils/display-path`):
 * the error copy that names it shows its plain path. Every runtime param goes
 * through `esc()`, so this pins that one step for every factory.
 */

import { describe, it, expect } from 'vitest'
import { getListingErrorMessage } from './listing-error-messages'

describe('error copy naming a twin', () => {
  it('shows the twin’s plain path', () => {
    const message = getListingErrorMessage({ reason: 'notFound', path: '/DCIM/\u00004242/photo.jpg' })

    expect(message.message).toContain('/DCIM/photo.jpg')
    expect(message.message).not.toContain('\u0000')
  })
})
