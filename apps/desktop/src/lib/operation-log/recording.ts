/**
 * Whether the operation log is recording new operations (`operationLog.enabled`).
 *
 * The one question every "Undo" surface asks before offering itself right after an
 * operation finishes (the trash toast and Edit > Undo trash, Undo rename, Ask Cmdr's
 * rename undo): an operation that ran while recording was off never reached the
 * journal, so its undo could only be refused. Read at completion time, which matches
 * the backend's per-operation decision except for an operation the switch flipped
 * under; that edge offers less (the history dialog still has it) or gets the
 * ordinary refusal notice, never a wrong undo.
 */

import { getSetting } from '#lib/settings/index.js'

export function isOperationLogRecording(): boolean {
  return getSetting('operationLog.enabled')
}
