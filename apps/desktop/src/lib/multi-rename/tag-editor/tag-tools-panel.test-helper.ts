/** A fake Tags tab state and scope runner for the three tool panels' tests. */

import { vi } from 'vitest'
import type { NamePreviewRow } from '#lib/tauri-commands/index.js'
import type { TagEditorState, TagStateError, TagStateResult } from './tag-editor-state.svelte'
import type { RunScoped } from './tag-tools'

export const SCOPE = ['/b/01.mp3', '/b/02.mp3']

export function fakeTags(rows: NamePreviewRow[] = []) {
  const tags = {
    error: null as TagStateError | null,
    busy: false,
    textTool: vi.fn<TagEditorState['textTool']>(() => Promise.resolve()),
    tool: vi.fn<TagEditorState['tool']>(() => Promise.resolve()),
    previewFromNames: vi.fn<TagEditorState['previewFromNames']>((): Promise<TagStateResult<NamePreviewRow[]>> =>
      Promise.resolve({ ok: true, value: rows }),
    ),
  }
  return tags
}

export const asTags = (fake: ReturnType<typeof fakeTags>): TagEditorState => fake as unknown as TagEditorState

/** Runs the call with the scope `scope` (the checked tracks), the way the Tags tab does. */
export type FakeRunScoped = RunScoped & { mockImplementation: (impl: () => Promise<undefined>) => unknown }

export function fakeRunScoped(scope: string[] | null = SCOPE): FakeRunScoped {
  return vi.fn((call: (scope: string[] | null) => Promise<unknown>) => call(scope)) as unknown as FakeRunScoped
}

export function buttonNamed(root: HTMLElement, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === text)
  if (!found) throw new Error(`no button ${text}`)
  return found
}

/** The checkbox inside the label reading `text`. */
export function boxNamed(root: HTMLElement, text: string): HTMLInputElement {
  const label = [...root.querySelectorAll('label')].find((l) => l.textContent.trim() === text)
  const box = label?.querySelector<HTMLInputElement>('input[type="checkbox"]')
  if (!box) throw new Error(`no checkbox ${text}`)
  return box
}
