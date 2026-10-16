// The `cmdr` terminal command: the app menu's install link in `/usr/local/bin`.
//
// macOS only (`src-tauri/src/cli/`), so each wrapper swallows the missing-command
// rejection other platforms give and answers `null` there.

import { commands } from '#lib/ipc/bindings.js'
import type { CliCommandResult, CliCommandStatus } from '#lib/ipc/bindings.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('cliCommand')

/**
 * Where the `cmdr` command stands, read through to the filesystem on every call.
 * `null` off macOS, or when the backend didn't answer.
 */
export async function getCliCommandStatus(): Promise<CliCommandStatus | null> {
  try {
    return await commands.getCliCommandStatus()
  } catch (error) {
    log.debug('The cmdr command status is unavailable: {error}', { error })
    return null
  }
}

/**
 * Put the `cmdr` command on PATH. `adminPrompt` is what macOS's password dialog says,
 * when `/usr/local/bin` needs one. Answers with the state it left things in.
 */
export async function installCliCommand(adminPrompt: string): Promise<CliCommandResult | null> {
  try {
    return await commands.installCliCommand(adminPrompt)
  } catch (error) {
    log.debug('Could not install the cmdr command: {error}', { error })
    return null
  }
}

/** Take the `cmdr` command off PATH, when it's ours to take. Same shape as `installCliCommand`. */
export async function uninstallCliCommand(adminPrompt: string): Promise<CliCommandResult | null> {
  try {
    return await commands.uninstallCliCommand(adminPrompt)
  } catch (error) {
    log.debug('Could not uninstall the cmdr command: {error}', { error })
    return null
  }
}
