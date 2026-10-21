/**
 * The remembered "files already exist" choice: what the dialog opens on, what a
 * confirm dispatches, what a confirm saves under "Remember this choice", and when
 * the overwrite note shows. The settings store is stubbed; the dialog wiring is covered in
 * `TransferDialog.remembered-policy.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { DefaultConflictPolicy } from '#lib/settings/index.js'

const stored: { value: unknown } = { value: 'stop' }
const setSettingMock = vi.fn<(id: string, value: unknown) => void>()

vi.mock('#lib/settings/index.js', () => ({
  getSetting: () => stored.value,
  setSetting: (id: string, value: unknown) => {
    setSettingMock(id, value)
  },
}))

import {
  dispatchedConflictPolicy,
  overwriteNoteReach,
  readRememberedConflictPolicy,
  rememberConflictPolicy,
  rememberStartsTicked,
  REMEMBERED_CONFLICT_POLICY_SETTING,
} from './remembered-conflict-policy'

beforeEach(() => {
  stored.value = 'stop'
  setSettingMock.mockReset()
})

describe('readRememberedConflictPolicy', () => {
  it('returns each of the five dialog choices as stored', () => {
    for (const value of ['stop', 'skip', 'overwrite', 'overwrite_smaller', 'overwrite_older'] as const) {
      stored.value = value
      expect(readRememberedConflictPolicy()).toBe(value)
    }
  })

  it('falls back to asking for anything it does not recognize', () => {
    for (const value of ['rename', 'overwrite_all', 500, undefined, null]) {
      stored.value = value
      expect(readRememberedConflictPolicy()).toBe('stop')
    }
  })
})

describe('dispatchedConflictPolicy', () => {
  it('sends a remembered policy only while its radios are on screen', () => {
    expect(dispatchedConflictPolicy({ policy: 'overwrite', source: 'remembered', choiceVisible: true })).toBe(
      'overwrite',
    )
    // Still checking, couldn't check, or nothing clashes: the person never saw it, so ask.
    expect(dispatchedConflictPolicy({ policy: 'overwrite', source: 'remembered', choiceVisible: false })).toBe('stop')
    expect(dispatchedConflictPolicy({ policy: 'skip', source: 'remembered', choiceVisible: false })).toBe('stop')
  })

  it('keeps a picked or an explicitly named policy whether or not the radios show', () => {
    expect(dispatchedConflictPolicy({ policy: 'skip', source: 'picked', choiceVisible: false })).toBe('skip')
    expect(dispatchedConflictPolicy({ policy: 'overwrite', source: 'explicit', choiceVisible: false })).toBe(
      'overwrite',
    )
  })
})

describe('rememberConflictPolicy', () => {
  /** A person's confirm with the radios on screen; each test overrides what it's about. */
  function confirm(overrides: Partial<Parameters<typeof rememberConflictPolicy>[0]>): void {
    rememberConflictPolicy({
      policy: 'skip',
      source: 'picked',
      remember: true,
      choiceVisible: true,
      operationType: 'copy',
      ...overrides,
    })
  }

  it('saves the selected policy when “Remember this choice” is ticked', () => {
    confirm({ policy: 'overwrite_older' })
    expect(setSettingMock).toHaveBeenCalledWith(REMEMBERED_CONFLICT_POLICY_SETTING, 'overwrite_older')
  })

  it('remembers a move’s choice too', () => {
    confirm({ policy: 'skip', operationType: 'move' })
    expect(setSettingMock).toHaveBeenCalledWith(REMEMBERED_CONFLICT_POLICY_SETTING, 'skip')
  })

  it('never makes a pick the new default without the tick', () => {
    confirm({ policy: 'overwrite', remember: false })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('forgets a saved choice when the person unticks the box', () => {
    stored.value = 'overwrite'
    confirm({ policy: 'overwrite', source: 'remembered', remember: false })
    expect(setSettingMock).toHaveBeenCalledWith(REMEMBERED_CONFLICT_POLICY_SETTING, 'stop')
  })

  it('saves a pick made on top of a remembered choice, tick kept', () => {
    stored.value = 'skip'
    confirm({ policy: 'overwrite_smaller', remember: true })
    expect(setSettingMock).toHaveBeenCalledWith(REMEMBERED_CONFLICT_POLICY_SETTING, 'overwrite_smaller')
  })

  it('saves “Ask later” when the person confirms on it, whatever the hidden box held', () => {
    stored.value = 'skip'
    confirm({ policy: 'stop', remember: true })
    expect(setSettingMock).toHaveBeenCalledWith(REMEMBERED_CONFLICT_POLICY_SETTING, 'stop')
  })

  it('writes nothing when the outcome matches the saved choice (sparse persistence stays sparse)', () => {
    stored.value = 'skip'
    confirm({ policy: 'skip', source: 'remembered', remember: true })
    stored.value = 'stop'
    confirm({ policy: 'overwrite', remember: false })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('writes nothing while the radios and the box are off screen', () => {
    stored.value = 'overwrite'
    confirm({ policy: 'overwrite', source: 'remembered', remember: false, choiceVisible: false })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('never writes an explicitly named (MCP) policy', () => {
    confirm({ policy: 'overwrite', source: 'explicit', remember: true })
    confirm({ policy: 'overwrite', source: 'explicit', remember: false })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('never writes from compress, which has no conflict choice', () => {
    confirm({ policy: 'overwrite', operationType: 'compress' })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('never writes a policy the setting cannot hold', () => {
    confirm({ policy: 'rename' })
    expect(setSettingMock).not.toHaveBeenCalled()
  })
})

describe('rememberStartsTicked', () => {
  it('ticks the box exactly when a choice other than “Ask later” is saved', () => {
    expect(rememberStartsTicked('stop')).toBe(false)
    for (const policy of ['skip', 'overwrite', 'overwrite_smaller', 'overwrite_older'] as const) {
      expect(rememberStartsTicked(policy)).toBe(true)
    }
  })
})

describe('overwriteNoteReach', () => {
  const cases: [DefaultConflictPolicy, ReturnType<typeof overwriteNoteReach>][] = [
    ['overwrite', 'all'],
    ['overwrite_smaller', 'smaller'],
    ['overwrite_older', 'older'],
    ['skip', null],
    ['stop', null],
  ]

  it.each(cases)('a remembered %s gives %s', (policy, reach) => {
    expect(overwriteNoteReach({ policy, source: 'remembered' })).toBe(reach)
  })

  it('stays quiet once the person picked, or for an explicitly named policy', () => {
    expect(overwriteNoteReach({ policy: 'overwrite', source: 'picked' })).toBeNull()
    expect(overwriteNoteReach({ policy: 'overwrite', source: 'explicit' })).toBeNull()
  })
})
