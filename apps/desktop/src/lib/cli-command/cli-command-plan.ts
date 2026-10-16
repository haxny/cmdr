/**
 * What a click on "Install “cmdr” command" does, and what the toast after it says.
 * Pure: every decision reads the typed state the backend reported, ❌ never a message.
 */
import type { CliCommandResult, CliCommandStatus } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { ToastLevel } from '#lib/ui/toast/index.js'

export type CliCommandAction = 'install' | 'uninstall'

export type CliCommandClick = { kind: CliCommandAction } | { kind: 'explain'; key: MessageKey }

export interface CliCommandFeedback {
  key: MessageKey
  level: ToastLevel
}

/**
 * The action a click takes, or the reason it can't take one. A dev build explains in
 * every state: it neither installs its own `target/` copy nor removes the installed
 * app's link.
 */
export function planCliCommandClick(status: CliCommandStatus): CliCommandClick {
  if (status.blockedBy === 'notProductionBuild') return { kind: 'explain', key: 'main.cliCommand.notProductionBuild' }
  switch (status.state) {
    case 'installed':
      return { kind: 'uninstall' }
    case 'installedByHomebrew':
      return { kind: 'explain', key: 'main.cliCommand.homebrew' }
    case 'takenByOther':
      return { kind: 'explain', key: 'main.cliCommand.takenByOther' }
    case 'notInstalled':
      return status.blockedBy === 'notInApplications'
        ? { kind: 'explain', key: 'main.cliCommand.notInApplications' }
        : { kind: 'install' }
  }
}

/** The toast for what an install or uninstall did. `null` means the backend didn't answer. */
export function cliCommandFeedback(action: CliCommandAction, result: CliCommandResult | null): CliCommandFeedback {
  if (!result) return { key: 'main.cliCommand.unavailable', level: 'warn' }
  switch (result.outcome.kind) {
    case 'installed':
      return { key: 'main.cliCommand.installed', level: 'success' }
    case 'uninstalled':
      return { key: 'main.cliCommand.uninstalled', level: 'success' }
    case 'unchanged':
      return {
        key: action === 'install' ? 'main.cliCommand.alreadyInstalled' : 'main.cliCommand.alreadyUninstalled',
        level: 'info',
      }
    case 'canceled':
      return { key: 'main.cliCommand.canceled', level: 'info' }
    case 'refused': {
      // The backend refused on what it found, so word it from that, not from the stale read.
      const click = planCliCommandClick(result.status)
      return click.kind === 'explain'
        ? { key: click.key, level: 'info' }
        : {
            key: action === 'install' ? 'main.cliCommand.installFailed' : 'main.cliCommand.uninstallFailed',
            level: 'warn',
          }
    }
    case 'failed':
      return {
        key: action === 'install' ? 'main.cliCommand.installFailed' : 'main.cliCommand.uninstallFailed',
        level: 'warn',
      }
  }
}
