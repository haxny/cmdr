import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { CliCommandResult, CliCommandStatus } from '#lib/ipc/bindings.js'

const m = vi.hoisted(() => ({
  getCliCommandStatus: vi.fn<() => Promise<CliCommandStatus | null>>(),
  installCliCommand: vi.fn<(prompt: string) => Promise<CliCommandResult | null>>(),
  uninstallCliCommand: vi.fn<(prompt: string) => Promise<CliCommandResult | null>>(),
  addToast: vi.fn<(...a: unknown[]) => void>(),
  updateCliCommandName: vi.fn<(state: string) => void>(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  getCliCommandStatus: () => m.getCliCommandStatus(),
  installCliCommand: (p: string) => m.installCliCommand(p),
  uninstallCliCommand: (p: string) => m.uninstallCliCommand(p),
}))
vi.mock('#lib/ui/toast/index.js', () => ({
  addToast: (...a: unknown[]) => {
    m.addToast(...a)
  },
}))
vi.mock('#lib/commands/command-registry.js', () => ({
  updateCliCommandName: (s: string) => {
    m.updateCliCommandName(s)
  },
}))
vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: (key: string) => key }))

import { refreshCliCommandName, toggleCliCommand } from './toggle-cli-command'

const status = (
  state: CliCommandStatus['state'],
  blockedBy: CliCommandStatus['blockedBy'] = null,
): CliCommandStatus => ({
  state,
  blockedBy,
})

const toastKey = (): unknown => m.addToast.mock.calls[0]?.[0]

beforeEach(() => {
  vi.clearAllMocks()
})

describe('toggleCliCommand', () => {
  it('installs when nothing is there, with the install prompt, and says so', async () => {
    m.getCliCommandStatus.mockResolvedValueOnce(status('notInstalled'))
    m.installCliCommand.mockResolvedValueOnce({ outcome: { kind: 'installed' }, status: status('installed') })

    await toggleCliCommand()

    expect(m.installCliCommand).toHaveBeenCalledWith('main.cliCommand.adminPromptInstall')
    expect(m.uninstallCliCommand).not.toHaveBeenCalled()
    expect(m.updateCliCommandName).toHaveBeenLastCalledWith('installed')
    expect(toastKey()).toBe('main.cliCommand.installed')
  })

  it('uninstalls our own link', async () => {
    m.getCliCommandStatus.mockResolvedValueOnce(status('installed'))
    m.uninstallCliCommand.mockResolvedValueOnce({ outcome: { kind: 'uninstalled' }, status: status('notInstalled') })

    await toggleCliCommand()

    expect(m.uninstallCliCommand).toHaveBeenCalledWith('main.cliCommand.adminPromptUninstall')
    expect(toastKey()).toBe('main.cliCommand.uninstalled')
  })

  it('explains without acting when Homebrew owns the command', async () => {
    m.getCliCommandStatus.mockResolvedValueOnce(status('installedByHomebrew'))

    await toggleCliCommand()

    expect(m.installCliCommand).not.toHaveBeenCalled()
    expect(m.uninstallCliCommand).not.toHaveBeenCalled()
    expect(toastKey()).toBe('main.cliCommand.homebrew')
  })

  it('says it could not check when the backend does not answer', async () => {
    m.getCliCommandStatus.mockResolvedValueOnce(null)

    await toggleCliCommand()

    expect(toastKey()).toBe('main.cliCommand.unavailable')
    expect(m.updateCliCommandName).not.toHaveBeenCalled()
  })

  it('reads a lost install answer as unavailable, leaving the label as last read', async () => {
    m.getCliCommandStatus.mockResolvedValueOnce(status('notInstalled'))
    m.installCliCommand.mockResolvedValueOnce(null)

    await toggleCliCommand()

    expect(m.updateCliCommandName).toHaveBeenCalledTimes(1)
    expect(toastKey()).toBe('main.cliCommand.unavailable')
  })
})

describe('refreshCliCommandName', () => {
  it('updates the palette label from a fresh read, and leaves it alone without one', async () => {
    m.getCliCommandStatus.mockResolvedValueOnce(status('installedByHomebrew'))
    await refreshCliCommandName()
    expect(m.updateCliCommandName).toHaveBeenCalledWith('installedByHomebrew')

    m.getCliCommandStatus.mockResolvedValueOnce(null)
    await refreshCliCommandName()
    expect(m.updateCliCommandName).toHaveBeenCalledTimes(1)
  })
})
