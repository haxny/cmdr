/**
 * MCP `dialog confirm`'s reply: the open dialog's Confirm (or, with
 * `startInBackground`, its Background button) is pressed with the round-trip's
 * id, and a press that went through replies NOTHING here: the start it led to
 * answers with its `operationId` (`transfer-dispatch.ts`). Only a press that
 * didn't go through answers here, typed (`refusal`), so the backend can say so
 * without reading a sentence.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import type { ExplorerAPI } from './explorer-api'
import type { ProgrammaticConfirmVerdict } from '$lib/file-explorer/pane/programmatic-confirm'
import { confirmDialogForMcp } from './mcp-dialog-confirm'

function explorerAnswering(verdict: ProgrammaticConfirmVerdict) {
  return { confirmDialog: vi.fn(() => Promise.resolve(verdict)) } as unknown as ExplorerAPI & {
    confirmDialog: ReturnType<typeof vi.fn>
  }
}

beforeEach(() => {
  vi.mocked(emit).mockClear()
  vi.mocked(emit).mockResolvedValue(undefined)
})

describe('a confirm over MCP', () => {
  it('presses the Background button with the round-trip id, and leaves the answer to the start', async () => {
    const explorer = explorerAnswering({ pressed: true })

    await confirmDialogForMcp({
      explorer,
      type: 'transfer-confirmation',
      onConflict: 'skip_all',
      startInBackground: true,
      requestId: 'req-1',
    })

    expect(explorer.confirmDialog).toHaveBeenCalledWith('transfer-confirmation', 'skip_all', {
      startInBackground: true,
      mcpRequestId: 'req-1',
    })
    expect(emit).not.toHaveBeenCalled()
  })

  it('presses a foreground Confirm with the round-trip id too', async () => {
    const explorer = explorerAnswering({ pressed: true })

    await confirmDialogForMcp({ explorer, type: 'delete-confirmation', requestId: 'req-5' })

    expect(explorer.confirmDialog).toHaveBeenCalledWith('delete-confirmation', undefined, {
      startInBackground: undefined,
      mcpRequestId: 'req-5',
    })
    expect(emit).not.toHaveBeenCalled()
  })

  it('carries the permanent-delete refusal as a typed field', async () => {
    const explorer = explorerAnswering({ pressed: false, refusal: 'permanentDelete' })

    await confirmDialogForMcp({ explorer, type: 'delete-confirmation', startInBackground: true, requestId: 'req-2' })

    expect(emit).toHaveBeenCalledWith('mcp-response', { requestId: 'req-2', ok: false, refusal: 'permanentDelete' })
  })

  it('says, typed, when the dialog took the press and started nothing', async () => {
    const explorer = explorerAnswering({ pressed: false, refusal: 'notStarted' })

    await confirmDialogForMcp({ explorer, type: 'transfer-confirmation', requestId: 'req-6' })

    expect(emit).toHaveBeenCalledWith('mcp-response', { requestId: 'req-6', ok: false, refusal: 'notStarted' })
  })

  it('fails the round-trip when no dialog was there to press', async () => {
    const explorer = explorerAnswering({ pressed: false, refusal: 'notReady' })

    await confirmDialogForMcp({ explorer, type: 'delete-confirmation', startInBackground: true, requestId: 'req-3' })

    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-3',
      ok: false,
      error: expect.any(String) as unknown,
    })
  })

  it('fails the round-trip when no explorer is mounted', async () => {
    await confirmDialogForMcp({
      explorer: undefined,
      type: 'transfer-confirmation',
      startInBackground: true,
      requestId: 'req-4',
    })

    expect(emit).toHaveBeenCalledWith('mcp-response', expect.objectContaining({ requestId: 'req-4', ok: false }))
  })
})
