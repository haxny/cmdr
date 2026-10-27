/** DOM helpers shared by the Tags tab's component tests. */

import { mount, tick, unmount } from 'svelte'
import type { Component } from 'svelte'

/** Mounts `component` into the document; the returned `cleanup` unmounts it. */
export async function mountInto<P extends Record<string, unknown>>(
  component: Component<P>,
  props: P,
): Promise<{ root: HTMLElement; cleanup: () => void }> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const instance = mount(component, { target: root, props })
  await tick()
  return {
    root,
    cleanup: () => {
      void unmount(instance)
      root.remove()
    },
  }
}

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
