/**
 * The Profiles menu's rows (F2 in Synchronize folders), the multi-rename presets pattern: the saved profiles
 * numbered 1–9, each with Rename / Move up / Move down / Delete, then Save as profile. A profile this build can't use
 * is listed greyed with its problem in words, never dropped. Built from data so the house `Menu` draws and drives it.
 */
import type { MenuItem, MenuSection } from '#lib/ui/menu-types.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type { ProfileMove, ProfileProblem } from '#lib/tauri-commands/sync.js'
import type { ProfileView } from './profiles.svelte'

/** Profiles past this many have no digit: the keys stop at 9. */
const NUMBERED = 9

export type ProfileAction =
  | { kind: 'load'; id: string }
  /** A greyed profile picked: say why it can't load (its submenu still renames, moves, or deletes it). */
  | { kind: 'invalid'; problem: ProfileProblem }
  | { kind: 'saveAs' }
  | { kind: 'rename'; id: string }
  | { kind: 'move'; id: string; direction: ProfileMove }
  | { kind: 'delete'; id: string }

/** Why a saved profile can't be used, in words (after its name and a colon). */
export function profileProblemText(problem: ProfileProblem): string {
  switch (problem.type) {
    case 'emptyName':
      return tString('sync.profiles.problem.emptyName')
    case 'badMask':
      return tString('sync.profiles.problem.badMask')
    case 'toleranceOutOfRange':
      return tString('sync.profiles.problem.toleranceOutOfRange')
    case 'percentOutOfRange':
      return tString('sync.profiles.problem.percentOutOfRange')
    case 'countOutOfRange':
      return tString('sync.profiles.problem.countOutOfRange')
    case 'badKeep':
      return tString('sync.profiles.problem.badKeep')
    case 'unreadable':
      return tString('sync.profiles.problem.unreadable')
    default: {
      const unreachable: never = problem
      return unreachable
    }
  }
}

function profileRow(view: ProfileView, index: number, total: number, loadedId: string | null): MenuItem<ProfileAction> {
  const { id, problem } = view
  const label = problem
    ? tString('sync.profiles.invalidRow', { name: view.name, problem: profileProblemText(problem) })
    : view.name
  return {
    value: `profile:${id}`,
    label,
    accelerator: index < NUMBERED ? String(index + 1) : undefined,
    check: id === loadedId ? { kind: 'current' } : undefined,
    // A greyed profile can't load, but it can still be renamed, moved, or deleted.
    data: problem ? { kind: 'invalid', problem } : { kind: 'load', id },
    tooltip: problem ? profileProblemText(problem) : undefined,
    submenu: [
      { value: `profile:${id}:rename`, label: tString('sync.profiles.rename'), data: { kind: 'rename', id } },
      {
        value: `profile:${id}:moveUp`,
        label: tString('sync.profiles.moveUp'),
        disabled: index === 0,
        data: { kind: 'move', id, direction: 'up' },
      },
      {
        value: `profile:${id}:moveDown`,
        label: tString('sync.profiles.moveDown'),
        disabled: index === total - 1,
        data: { kind: 'move', id, direction: 'down' },
      },
      { value: `profile:${id}:delete`, label: tString('sync.profiles.delete'), data: { kind: 'delete', id } },
    ],
  }
}

export function profileMenuSections(views: ProfileView[], loadedId: string | null): MenuSection<ProfileAction>[] {
  const sections: MenuSection<ProfileAction>[] = []
  if (views.length > 0) {
    sections.push({
      id: 'saved',
      heading: tString('sync.profiles.saved'),
      items: views.map((view, index) => profileRow(view, index, views.length, loadedId)),
    })
  }
  sections.push({
    id: 'actions',
    items: [{ value: 'saveAs', label: tString('sync.profiles.saveAs'), data: { kind: 'saveAs' } }],
  })
  return sections
}
