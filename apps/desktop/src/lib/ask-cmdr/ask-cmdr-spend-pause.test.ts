/**
 * The spend pause in the rail: a running answer whose estimated spend passed the user's threshold
 * asks "keep going?", live and after a reload, and Keep going, Stop, and the turn ending each take
 * the question off screen.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent } from '#lib/tauri-commands/index.js'
import type { CancelOutcomeView } from '#lib/ipc/bindings.js'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const cancelMock = vi.fn<(id: number) => Promise<CancelOutcomeView>>()
const keepGoingMock = vi.fn<(id: number) => Promise<boolean>>()
const getMock = vi.fn<(...a: unknown[]) => Promise<unknown>>()

vi.mock('#lib/tauri-commands/index.js', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: (id: number) => cancelMock(id),
  keepGoingAskCmdr: (id: number) => keepGoingMock(id),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: (...a: unknown[]) => getMock(...a),
  recordAskCmdrSlotChange: vi.fn(() => Promise.resolve([])),
  preflightBulkRename: vi.fn(() => Promise.resolve({ status: 'ready', rows: [] })),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  reviseBulkRenameRow: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/app-status-store.js', () => ({ saveAppStatus: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('#lib/file-explorer/pane/explorer-state.svelte.js', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: vi.fn(() => Promise.resolve()),
  shrinkMainWindowForRail: vi.fn(() => Promise.resolve()),
}))
vi.mock('./ask-cmdr-gate.svelte', () => ({
  refreshRailGate: vi.fn(() => Promise.resolve('chat')),
}))

import {
  askCmdrState,
  keepGoingPastSpend,
  newChat,
  sendMessage,
  stopStreaming,
  switchToThread,
} from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetTurnBookkeeping } from './ask-cmdr-stream.svelte'

const THREAD_ID = 1

function fire(event: AskCmdrStreamEvent): void {
  askCmdrState.conversationId = THREAD_ID
  handleTurnEvent({ conversationId: THREAD_ID, run: null, event })
}

/** A turn that ran a round and then paused at $2.40. */
function pausedTurn(): void {
  sendMessage('clean up everything')
  fire({ type: 'started' })
  fire({ type: 'assistantStarted' })
  fire({ type: 'textDelta', text: 'Looking.' })
  fire({ type: 'spendPaused', costMicros: 2_400_000 })
}

beforeEach(() => {
  sendMock.mockReset()
  cancelMock.mockReset()
  keepGoingMock.mockReset()
  getMock.mockReset()
  sendMock.mockImplementation((c) => Promise.resolve({ accepted: true, conversationId: c ?? THREAD_ID, run: 1 }))
  cancelMock.mockResolvedValue({ tripped: 1, live: 1 })
  keepGoingMock.mockResolvedValue(true)
  newChat()
  resetTurnBookkeeping()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
})

describe('the spend pause', () => {
  it('puts the question on screen while the turn waits', () => {
    pausedTurn()
    expect(askCmdrState.streaming).toBe(true)
    expect(askCmdrState.spendPausedMicros).toBe(2_400_000)
  })

  /** The turn waits on the user's word, so it may be quiet for as long as they take: the
   *  progress watchdog must not call that a timeout. */
  it('never times out while the question waits', async () => {
    vi.useFakeTimers()
    try {
      pausedTurn()
      await vi.advanceTimersByTimeAsync(30 * 60_000)
      expect(askCmdrState.streaming).toBe(true)
      expect(askCmdrState.messages.some((m) => m.kind === 'error')).toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })

  it('Keep going asks the backend to run the turn on and takes the question away', () => {
    pausedTurn()
    keepGoingPastSpend()
    expect(keepGoingMock).toHaveBeenCalledWith(THREAD_ID)
    expect(askCmdrState.spendPausedMicros).toBeNull()
    expect(askCmdrState.streaming).toBe(true)
  })

  it('Stop ends the turn and the question with it', () => {
    pausedTurn()
    stopStreaming()
    expect(cancelMock).toHaveBeenCalledWith(THREAD_ID)
    expect(askCmdrState.spendPausedMicros).toBeNull()
    expect(askCmdrState.streaming).toBe(false)
  })

  it('a resume from elsewhere, or the turn ending, clears it too', () => {
    pausedTurn()
    fire({ type: 'spendResumed' })
    expect(askCmdrState.spendPausedMicros).toBeNull()

    fire({ type: 'spendPaused', costMicros: 4_100_000 })
    fire({ type: 'stopped', turn: null })
    expect(askCmdrState.spendPausedMicros).toBeNull()
  })

  /** A paused turn sends nothing until the user answers, so the reload is what brings the
   *  question back. */
  it('comes back after a reload of a thread whose turn is paused', async () => {
    getMock.mockResolvedValue({
      conversation: { id: 9, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null },
      totalMessages: 0,
      messages: [],
      lastContextUsage: null,
      turnInFlightSinceSeq: 0,
      turnInFlightRun: 1,
      spendPausedMicros: 2_400_000,
    })
    await switchToThread(9)
    expect(askCmdrState.streaming).toBe(true)
    expect(askCmdrState.spendPausedMicros).toBe(2_400_000)
  })
})
