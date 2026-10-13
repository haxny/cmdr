import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { createRawSnippet } from 'svelte'
import Button from './Button.svelte'

/** Creates a text snippet for use as button children in tests. */
function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span>${text}</span>`,
  }))
}

function getButton(target: HTMLElement): HTMLButtonElement {
  const button = target.querySelector('button')
  if (!button) throw new Error('Button element not found')
  return button
}

describe('Button', () => {
  it('renders with default props (secondary, regular)', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { children: textSnippet('Click me') },
    })
    await tick()

    const button = getButton(target)
    expect(button.textContent).toContain('Click me')
    expect(button.type).toBe('button')
    expect(button.disabled).toBe(false)
    expect(button.classList.contains('btn-secondary')).toBe(true)
    expect(button.classList.contains('btn-regular')).toBe(true)
  })

  it('renders primary variant', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { variant: 'primary', children: textSnippet('Save') },
    })
    await tick()

    const button = getButton(target)
    expect(button.classList.contains('btn-primary')).toBe(true)
    expect(button.classList.contains('btn-regular')).toBe(true)
  })

  it('renders danger variant', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { variant: 'danger', children: textSnippet('Delete') },
    })
    await tick()

    const button = getButton(target)
    expect(button.classList.contains('btn-danger')).toBe(true)
  })

  it('renders mini size', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { size: 'mini', children: textSnippet('Small') },
    })
    await tick()

    const button = getButton(target)
    expect(button.classList.contains('btn-mini')).toBe(true)
  })

  it('renders disabled state', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { disabled: true, children: textSnippet('Disabled') },
    })
    await tick()

    const button = getButton(target)
    expect(button.disabled).toBe(true)
  })

  it('fires click events when not disabled', async () => {
    const target = document.createElement('div')
    const handleClick = vi.fn()
    mount(Button, {
      target,
      props: { onclick: handleClick, children: textSnippet('Click') },
    })
    await tick()

    const button = getButton(target)
    button.click()
    expect(handleClick).toHaveBeenCalledOnce()
  })

  it('does not fire click events when disabled', async () => {
    const target = document.createElement('div')
    const handleClick = vi.fn()
    mount(Button, {
      target,
      props: { disabled: true, onclick: handleClick, children: textSnippet('No click') },
    })
    await tick()

    const button = getButton(target)
    button.click()
    expect(handleClick).not.toHaveBeenCalled()
  })

  it('sets type attribute to submit when specified', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { type: 'submit', children: textSnippet('Submit') },
    })
    await tick()

    const button = getButton(target)
    expect(button.type).toBe('submit')
  })

  it('combines variant and size classes correctly', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { variant: 'primary', size: 'mini', children: textSnippet('Mini primary') },
    })
    await tick()

    const button = getButton(target)
    expect(button.classList.contains('btn-primary')).toBe(true)
    expect(button.classList.contains('btn-mini')).toBe(true)
  })

  it('draws a key hint in its own hidden slot after the label, and keeps it out of the name', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { keyHint: 'F2', 'aria-keyshortcuts': 'F2', children: textSnippet('Background') },
    })
    await tick()

    const button = getButton(target)
    expect(button.classList.contains('btn-keyed')).toBe(true)
    const [label, slot] = [...button.children]
    expect(label.classList.contains('btn-label')).toBe(true)
    expect(label.textContent).toBe('Background')
    expect(slot.classList.contains('btn-key-hint')).toBe(true)
    expect(slot.getAttribute('aria-hidden')).toBe('true')
    expect(slot.textContent).toBe('F2')
    expect(button.getAttribute('aria-keyshortcuts')).toBe('F2')
  })

  it('leaves the slot empty for a command with no binding, so it adds no gap', async () => {
    const target = document.createElement('div')
    mount(Button, {
      target,
      props: { keyHint: { commandId: 'app.about' }, children: textSnippet('About') },
    })
    await tick()

    const slot = getButton(target).querySelector('.btn-key-hint')
    expect(slot?.matches(':empty')).toBe(true)
  })

  it('renders its content alone without a key hint', async () => {
    const target = document.createElement('div')
    mount(Button, { target, props: { children: textSnippet('Plain') } })
    await tick()

    const button = getButton(target)
    expect(button.classList.contains('btn-keyed')).toBe(false)
    expect(button.querySelector('.btn-label, .btn-key-hint')).toBeNull()
  })
})
