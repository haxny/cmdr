/** File > "Edit tags…" follows the tag editor setting: greyed while it's off, live while it's on, live-updated. */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const flags = vi.hoisted(() => ({ on: false, listener: null as ((value: boolean) => void) | null }))
const setEditTagsEnabled = vi.hoisted(() => vi.fn(() => Promise.resolve()))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: () => flags.on,
  onSpecificSettingChange: (_id: string, listener: (value: boolean) => void) => {
    flags.listener = listener
    return () => {
      flags.listener = null
    }
  },
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ setEditTagsEnabled }))

import { startEditTagsMenuGate } from './edit-tags-menu-gate'

describe('the Edit tags menu gate', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    flags.on = false
  })

  it('greys the item at startup with the setting off, and follows the setting', () => {
    const stop = startEditTagsMenuGate()
    expect(setEditTagsEnabled).toHaveBeenLastCalledWith(false)
    flags.listener?.(true)
    expect(setEditTagsEnabled).toHaveBeenLastCalledWith(true)
    stop()
    expect(flags.listener).toBeNull()
  })

  it('enables the item at startup with the setting on', () => {
    flags.on = true
    startEditTagsMenuGate()()
    expect(setEditTagsEnabled).toHaveBeenCalledWith(true)
  })
})
