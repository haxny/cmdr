/** DOM helpers shared by the Tags window's component tests. */

import { mount, tick, unmount } from 'svelte'
import type { Component } from 'svelte'

/** Mounts `component` into the document; the returned `cleanup` unmounts it. */
export async function mountInto<P extends Record<string, unknown>>(
  component: Component<P>,
  props: P,
): Promise<{ root: HTMLElement; cleanup: () => void; instance: Record<string, unknown> }> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const instance = mount(component, { target: root, props })
  await tick()
  return {
    root,
    instance,
    cleanup: () => {
      void unmount(instance)
      root.remove()
    },
  }
}

// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- the type argument names the element a test expects
export function byId<T extends HTMLElement>(root: HTMLElement, id: string): T {
  const el = root.querySelector<T>(`#${id}`)
  if (!el) throw new Error(`no #${id}`)
  return el
}

/** Types `text` into a box the way a user does: focus, set the value, fire `input`. */
export async function typeInto(el: HTMLInputElement | HTMLTextAreaElement, text: string): Promise<void> {
  el.focus()
  el.value = text
  el.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
}

export async function press(el: HTMLElement, key: string, init: KeyboardEventInit = {}): Promise<KeyboardEvent> {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  el.dispatchEvent(event)
  await tick()
  return event
}

/** Leaves the box: a real blur, so the `blur` listener runs. */
export async function leave(el: HTMLElement): Promise<void> {
  el.blur()
  el.dispatchEvent(new FocusEvent('blur'))
  await tick()
}

/** The Tracks pane's row for `path` (`TracksPane`). */
export function trackRow(root: HTMLElement, path: string): HTMLElement {
  const row = root.querySelector<HTMLElement>(`[role="option"][data-track-path="${path}"]`)
  if (!row) throw new Error(`no track row ${path}`)
  return row
}

/** Clicks a track's ☐ in the Tracks pane: toggles its selection, as a click on a file's mark would. */
export async function toggleTrack(root: HTMLElement, path: string): Promise<void> {
  trackRow(root, path)
    .querySelector('[data-col="mark"]')
    ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await tick()
}

/** Opens a pane's in-place editor on its cursor row with F2 (`tags.edit`), and returns it. */
export async function editCursorRow(list: HTMLElement): Promise<HTMLInputElement | HTMLTextAreaElement> {
  await press(list, 'F2')
  const editor = list.parentElement?.querySelector<HTMLInputElement | HTMLTextAreaElement>('.tag-value-editor')
  if (!editor) throw new Error('no editor')
  return editor
}

/** A pane's listbox (`AlbumPane` / `TracksPane`), by its accessible name. */
export function paneList(root: HTMLElement, label: string): HTMLElement {
  const list = root.querySelector<HTMLElement>(`[role="listbox"][aria-label="${label}"]`)
  if (!list) throw new Error(`no ${label} pane`)
  return list
}
