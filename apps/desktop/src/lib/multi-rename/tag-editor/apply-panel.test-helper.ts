/** A fake Tags window state and a view for the Apply panel's tests. */

import { vi } from 'vitest'
import type { FileChangeView, TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'
import type { TagEditorState } from './tag-editor-state.svelte'
import type { LastTagApply } from './last-tag-apply'

export const PANEL_VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [],
  tracks: ['01.mp3', '02.mp3', '03.mp3'].map((fileName, i) => ({
    path: `/b/${fileName}`,
    fileName,
    track: String(i + 1),
    title: null,
    durationMs: null,
    changed: true,
    readError: null,
  })),
  cover: { kind: 'empty' },
  extras: [],
  dirty: true,
  diacritics: [],
  badDates: 0,
  sort: null,
}

export const CHANGES: FileChangeView[] = [
  { path: '/b/01.mp3', fileName: '01.mp3', fields: ['album', 'title'], cover: true, extras: false, removes: [] },
  { path: '/b/02.mp3', fileName: '02.mp3', fields: ['album'], cover: false, extras: true, removes: [] },
]

export function applyReport(over: Partial<TagApplyReport> = {}): TagApplyReport {
  return { applyId: 'A1', written: [], wouldDrop: [], failed: [], changedOnDisk: [], undoUnavailable: false, ...over }
}

/** A state whose calls answer at once; `apply` answers `reports` in turn. */
export function fakeState(view: TagEditorView = PANEL_VIEW, reports: TagApplyReport[] = [applyReport()]) {
  const queue = [...reports]
  return {
    view,
    error: null,
    busy: false,
    writing: false,
    editing: false,
    undoable: null as LastTagApply | null,
    settled: vi.fn(() => Promise.resolve(true)),
    changes: vi.fn(() => Promise.resolve({ ok: true as const, value: CHANGES })),
    apply: vi.fn(() => Promise.resolve({ ok: true as const, value: queue.shift() ?? applyReport() })),
    tool: vi.fn(() => Promise.resolve()),
    reload: vi.fn(() => Promise.resolve()),
    undo: vi.fn(() =>
      Promise.resolve({ ok: true as const, value: { restored: [], missing: [], changed: [], failed: [] } }),
    ),
  }
}

export const asState = (fake: ReturnType<typeof fakeState>): TagEditorState => fake as unknown as TagEditorState

export async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await new Promise((resolve) => setTimeout(resolve, 0))
}

/** A button's own words, without its key chip. */
export function labelOf(button: HTMLButtonElement): string {
  return [...button.childNodes]
    .filter((node) => !(node instanceof Element && node.classList.contains('btn-key-hint')))
    .map((node) => node.textContent)
    .join('')
    .trim()
}

export function buttonNamed(root: HTMLElement, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll('button')].find((b) => labelOf(b) === text)
  if (!found) throw new Error(`no button ${text}`)
  return found
}

export async function press(root: HTMLElement, text: string): Promise<void> {
  buttonNamed(root, text).click()
  await settle()
}
