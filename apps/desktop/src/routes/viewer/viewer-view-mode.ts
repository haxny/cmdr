import type { CommandId } from '#lib/commands/index.js'
import { eventMatchesCommand } from '#lib/shortcuts/index.js'

export type ViewerDisplayMode = 'text' | 'binary' | 'hex' | 'media'

/** Each mode's command. Defaults are Total Commander Lister's 0–3; the registry lets the user rebind them. */
const MODE_COMMANDS: readonly { mode: ViewerDisplayMode; command: CommandId }[] = [
  { mode: 'media', command: 'viewer.viewAsMedia' },
  { mode: 'text', command: 'viewer.viewAsText' },
  { mode: 'binary', command: 'viewer.viewAsBinary' },
  { mode: 'hex', command: 'viewer.viewAsHex' },
]

/**
 * The mode a keypress asks for, or `null`. Media only when the file has a rendered view. The
 * whole-combo match leaves a modified digit to the focused control or the OS.
 */
export function modeForKey(event: KeyboardEvent, mediaAvailable: boolean): ViewerDisplayMode | null {
  if (event.isComposing) return null
  const match = MODE_COMMANDS.find(({ command }) => eventMatchesCommand(event, command))
  if (!match || (match.mode === 'media' && !mediaAvailable)) return null
  return match.mode
}
