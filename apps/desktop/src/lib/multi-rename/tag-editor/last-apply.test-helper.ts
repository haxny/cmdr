/**
 * Tests' stand-in for the backend's last write per folder (`tag_editor/last_apply.rs`): `lastTagApply` answers what
 * the backend would hold after the `applyTags` and `undoTagApply` calls a test's mocks have answered so far, replayed
 * in call order. The window only mirrors it, so a test that writes and then undoes goes through the same steps.
 */
import type { Mock } from 'vitest'
import type { LastTagApply, RestoreReport, TagApplyReport } from '#lib/tauri-commands/index.js'

type Answer<T> = { ok: true; value: T } | { ok: false; error: unknown }

interface IpcWithWrites {
  applyTags: Mock
  undoTagApply: Mock
}

type Step =
  | { order: number; kind: 'apply'; continues: boolean; answer: Promise<Answer<TagApplyReport>> | undefined }
  | { order: number; kind: 'undo'; applyId: string; answer: Promise<Answer<RestoreReport>> | undefined }

function afterApply(last: LastTagApply | null, report: TagApplyReport, continues: boolean): LastTagApply | null {
  const wrote = report.written.length > 0
  if (!continues && !wrote) return last
  const before = continues && last !== null ? last : { applies: [], partial: false }
  const kept = report.applyId !== null && wrote && !report.undoUnavailable
  const next = {
    applies: kept
      ? [...before.applies, { applyId: report.applyId ?? '', written: report.written.length }]
      : before.applies,
    partial: before.partial || (report.undoUnavailable && wrote),
  }
  return next.applies.length > 0 || next.partial ? next : null
}

function afterUndo(last: LastTagApply | null, applyId: string, left: number): LastTagApply | null {
  if (last === null || !last.applies.some((apply) => apply.applyId === applyId)) return last
  const applies =
    left === 0
      ? last.applies.filter((apply) => apply.applyId !== applyId)
      : last.applies.map((apply) => (apply.applyId === applyId ? { ...apply, written: left } : apply))
  return applies.length > 0 ? { ...last, applies } : null
}

/** A `lastTagApply` implementation replaying `ipc`'s answered writes and undos. */
export function replayLastApply(ipc: IpcWithWrites): () => Promise<Answer<LastTagApply | null>> {
  return async () => {
    const steps: Step[] = [
      ...ipc.applyTags.mock.calls.map((call, i): Step => ({
        order: ipc.applyTags.mock.invocationCallOrder[i] ?? 0,
        kind: 'apply',
        continues: call[2] === true,
        answer: ipc.applyTags.mock.results[i]?.value as Promise<Answer<TagApplyReport>> | undefined,
      })),
      ...ipc.undoTagApply.mock.calls.map((call, i): Step => ({
        order: ipc.undoTagApply.mock.invocationCallOrder[i] ?? 0,
        kind: 'undo',
        applyId: String(call[0]),
        answer: ipc.undoTagApply.mock.results[i]?.value as Promise<Answer<RestoreReport>> | undefined,
      })),
    ].sort((a, b) => a.order - b.order)
    let last: LastTagApply | null = null
    for (const step of steps) {
      if (step.kind === 'apply') {
        const res = await step.answer
        if (res?.ok === true) last = afterApply(last, res.value, step.continues)
      } else {
        const res = await step.answer
        if (res?.ok === true) last = afterUndo(last, step.applyId, res.value.failed.length)
      }
    }
    return { ok: true, value: last }
  }
}
