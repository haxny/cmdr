/**
 * Tests for the archive-password Tauri command wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    unlockContainer: vi.fn(),
    forgetContainerPassword: vi.fn(),
    notifyArchivePasswordPrompt: vi.fn(),
    notifyArchivePasswordDismissed: vi.fn(),
  },
}))

import { commands } from '#lib/ipc/bindings.js'
import {
  setArchivePassword,
  clearArchivePassword,
  notifyArchivePasswordPrompt,
  notifyArchivePasswordDismissed,
} from './archive'

describe('setArchivePassword wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the volume id, archive path, and password, remembering nothing', async () => {
    vi.mocked(commands.unlockContainer).mockResolvedValueOnce({ status: 'ok', data: 'stored' })
    await setArchivePassword('root', '/a/secret.zip/inner/x.pdf', 'hunter2')
    expect(commands.unlockContainer).toHaveBeenCalledWith('root', '/a/secret.zip/inner/x.pdf', 'hunter2', false)
  })

  it('throws the typed reason on an error result', async () => {
    vi.mocked(commands.unlockContainer).mockResolvedValueOnce({ status: 'error', error: { type: 'notLocked' } })
    await expect(setArchivePassword('root', '/a/plain.txt', 'x')).rejects.toThrow('notLocked')
  })
})

describe('clearArchivePassword wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the volume id and archive path', async () => {
    vi.mocked(commands.forgetContainerPassword).mockResolvedValueOnce({ status: 'ok', data: null })
    await clearArchivePassword('root', '/a/secret.zip')
    expect(commands.forgetContainerPassword).toHaveBeenCalledWith('root', '/a/secret.zip')
  })

  it('throws the typed reason on an error result', async () => {
    vi.mocked(commands.forgetContainerPassword).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'volumeNotFound', volumeId: 'root' },
    })
    await expect(clearArchivePassword('root', '/a/secret.zip')).rejects.toThrow('volumeNotFound')
  })
})

describe('archive-password prompt mirror', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards what the prompt is asking, and nothing more', async () => {
    // The mirror is what lets `cmdr://state` name the archive. ❌ There is no
    // password field on it, and there must never be one: the secret's only path
    // is `setArchivePassword` (or, over MCP, the backend's own store).
    const prompt = {
      archiveName: 'secret.zip',
      archivePath: '/a/secret.zip/inner/x.pdf',
      parentVolumeId: 'root',
      container: 'archive' as const,
      mode: 'transfer' as const,
      wrongAttempt: true,
      operationId: 'op-3',
    }
    await notifyArchivePasswordPrompt(prompt)
    expect(commands.notifyArchivePasswordPrompt).toHaveBeenCalledWith(prompt)
    expect(JSON.stringify(prompt)).not.toContain('password')
  })

  it('clears the mirror when the prompt goes', async () => {
    await notifyArchivePasswordDismissed()
    expect(commands.notifyArchivePasswordDismissed).toHaveBeenCalledOnce()
  })
})
