import { describe, expect, it } from 'vitest'
import type { CliCommandResult, CliCommandStatus } from '#lib/ipc/bindings.js'
import { cliCommandFeedback, planCliCommandClick } from './cli-command-plan'

const status = (
  state: CliCommandStatus['state'],
  blockedBy: CliCommandStatus['blockedBy'] = null,
): CliCommandStatus => ({
  state,
  blockedBy,
})

describe('planCliCommandClick', () => {
  it('installs when nothing is there and uninstalls our own link', () => {
    expect(planCliCommandClick(status('notInstalled'))).toEqual({ kind: 'install' })
    expect(planCliCommandClick(status('installed'))).toEqual({ kind: 'uninstall' })
  })

  it('explains instead of acting when the click would be refused', () => {
    expect(planCliCommandClick(status('installedByHomebrew'))).toEqual({
      kind: 'explain',
      key: 'main.cliCommand.homebrew',
    })
    expect(planCliCommandClick(status('takenByOther'))).toEqual({
      kind: 'explain',
      key: 'main.cliCommand.takenByOther',
    })
    expect(planCliCommandClick(status('notInstalled', 'notInApplications'))).toEqual({
      kind: 'explain',
      key: 'main.cliCommand.notInApplications',
    })
  })

  it('a dev build explains whatever the state, since it acts in neither direction', () => {
    for (const state of ['notInstalled', 'installed', 'installedByHomebrew', 'takenByOther'] as const) {
      expect(planCliCommandClick(status(state, 'notProductionBuild'))).toEqual({
        kind: 'explain',
        key: 'main.cliCommand.notProductionBuild',
      })
    }
  })
})

describe('cliCommandFeedback', () => {
  const result = (outcome: CliCommandResult['outcome'], state: CliCommandStatus['state']): CliCommandResult => ({
    outcome,
    status: status(state),
  })

  it('says what happened, in the direction the click went', () => {
    expect(cliCommandFeedback('install', result({ kind: 'installed' }, 'installed'))).toEqual({
      key: 'main.cliCommand.installed',
      level: 'success',
    })
    expect(cliCommandFeedback('uninstall', result({ kind: 'uninstalled' }, 'notInstalled'))).toEqual({
      key: 'main.cliCommand.uninstalled',
      level: 'success',
    })
    expect(cliCommandFeedback('install', result({ kind: 'unchanged' }, 'installed'))).toEqual({
      key: 'main.cliCommand.alreadyInstalled',
      level: 'info',
    })
    expect(cliCommandFeedback('uninstall', result({ kind: 'unchanged' }, 'notInstalled'))).toEqual({
      key: 'main.cliCommand.alreadyUninstalled',
      level: 'info',
    })
    expect(cliCommandFeedback('install', result({ kind: 'canceled' }, 'notInstalled'))).toEqual({
      key: 'main.cliCommand.canceled',
      level: 'info',
    })
    expect(cliCommandFeedback('install', result({ kind: 'failed', code: 1 }, 'notInstalled'))).toEqual({
      key: 'main.cliCommand.installFailed',
      level: 'warn',
    })
    expect(cliCommandFeedback('uninstall', result({ kind: 'failed', code: null }, 'installed'))).toEqual({
      key: 'main.cliCommand.uninstallFailed',
      level: 'warn',
    })
  })

  it('a refusal is worded from the state it reports, which can have moved since the click', () => {
    // Someone else's `cmdr` appeared between the status read and the install.
    expect(cliCommandFeedback('install', result({ kind: 'refused' }, 'takenByOther'))).toEqual({
      key: 'main.cliCommand.takenByOther',
      level: 'info',
    })
  })

  it('no answer at all reads as unavailable', () => {
    expect(cliCommandFeedback('install', null)).toEqual({ key: 'main.cliCommand.unavailable', level: 'warn' })
  })
})
