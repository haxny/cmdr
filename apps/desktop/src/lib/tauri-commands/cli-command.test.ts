/**
 * The `cmdr` command wrappers pass the backend's typed answer through, and read a
 * missing command (every non-macOS platform) or a dead backend as `null`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    getCliCommandStatus: vi.fn(),
    installCliCommand: vi.fn(),
    uninstallCliCommand: vi.fn(),
  },
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { commands } from '#lib/ipc/bindings.js'
import { getCliCommandStatus, installCliCommand, uninstallCliCommand } from './cli-command'

const installed = { state: 'installed', blockedBy: null } as const

beforeEach(() => {
  vi.clearAllMocks()
})

describe('cmdr command wrappers', () => {
  it('pass the status and the results straight through', async () => {
    vi.mocked(commands.getCliCommandStatus).mockResolvedValueOnce(installed)
    expect(await getCliCommandStatus()).toEqual(installed)

    const result = { outcome: { kind: 'installed' }, status: installed } as const
    vi.mocked(commands.installCliCommand).mockResolvedValueOnce(result)
    expect(await installCliCommand('prompt')).toEqual(result)
    expect(commands.installCliCommand).toHaveBeenCalledWith('prompt')

    vi.mocked(commands.uninstallCliCommand).mockResolvedValueOnce(result)
    expect(await uninstallCliCommand('prompt')).toEqual(result)
  })

  it('answer null where the command does not exist', async () => {
    vi.mocked(commands.getCliCommandStatus).mockRejectedValueOnce(new Error('no such command'))
    vi.mocked(commands.installCliCommand).mockRejectedValueOnce(new Error('no such command'))
    vi.mocked(commands.uninstallCliCommand).mockRejectedValueOnce(new Error('no such command'))
    expect(await getCliCommandStatus()).toBeNull()
    expect(await installCliCommand('prompt')).toBeNull()
    expect(await uninstallCliCommand('prompt')).toBeNull()
  })
})
