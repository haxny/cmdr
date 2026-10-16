/**
 * The app menu's "Install “cmdr” command" item and its palette twin: read where the
 * command stands, do what the item offered, and say how it went.
 */
import { updateCliCommandName } from '#lib/commands/command-registry.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getCliCommandStatus, installCliCommand, uninstallCliCommand } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { cliCommandFeedback, planCliCommandClick } from './cli-command-plan'

const TOAST_ID = 'cli-command'
const TOAST_TIMEOUT_MS = 8000

/** Re-read the command's state for the palette label. The native menu item refreshes itself in Rust. */
export async function refreshCliCommandName(): Promise<void> {
  const status = await getCliCommandStatus()
  if (status) updateCliCommandName(status.state)
}

export async function toggleCliCommand(): Promise<void> {
  const status = await getCliCommandStatus()
  if (!status) {
    addToast(tString('main.cliCommand.unavailable'), { id: TOAST_ID, level: 'warn', timeoutMs: TOAST_TIMEOUT_MS })
    return
  }
  updateCliCommandName(status.state)
  const click = planCliCommandClick(status)
  if (click.kind === 'explain') {
    addToast(tString(click.key), { id: TOAST_ID, level: 'info', timeoutMs: TOAST_TIMEOUT_MS })
    return
  }
  const result =
    click.kind === 'install'
      ? await installCliCommand(tString('main.cliCommand.adminPromptInstall'))
      : await uninstallCliCommand(tString('main.cliCommand.adminPromptUninstall'))
  if (result) updateCliCommandName(result.status.state)
  const feedback = cliCommandFeedback(click.kind, result)
  addToast(tString(feedback.key), { id: TOAST_ID, level: feedback.level, timeoutMs: TOAST_TIMEOUT_MS })
}
