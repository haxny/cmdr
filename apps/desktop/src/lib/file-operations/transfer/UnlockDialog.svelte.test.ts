/**
 * Rendering + interaction tests for `UnlockDialog.svelte`.
 *
 * The dialog asks for the password of a locked container: an encrypted archive
 * (before a copy/move out of it, or a listing of a header-encrypted one) or a
 * restic backup repo. Two shapes per container: the first prompt, and the
 * re-prompt after a rejected password (`wrongAttempt`), which shows distinct
 * copy and starts with an empty field. A repo also offers "Remember in
 * Keychain" and shows a busy state while the password is checked. Tauri's
 * dialog-tracking IPC is stubbed so it mounts in happy-dom.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import UnlockDialog from './UnlockDialog.svelte'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

type DialogProps = Parameters<typeof UnlockDialog>[1]

function findButton(target: HTMLElement, text: string): HTMLButtonElement {
  const btn = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === text)
  if (!btn) throw new Error(`Button "${text}" not found`)
  return btn
}

async function settle(): Promise<void> {
  // The field focuses a tick after mount; give it a couple of turns.
  await tick()
  await tick()
}

function passwordInput(target: HTMLElement): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>('input[type="password"]')
  if (!input) throw new Error('No password field')
  return input
}

async function type(target: HTMLElement, text: string): Promise<void> {
  const input = passwordInput(target)
  input.value = text
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
}

function rememberBox(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector<HTMLInputElement>('input[type="checkbox"]')
}

let target: HTMLElement

beforeEach(() => {
  target = document.createElement('div')
  document.body.appendChild(target)
})

const archiveProps = (overrides: Partial<DialogProps> = {}): DialogProps => ({
  container: 'archive',
  name: 'photos.zip',
  wrongAttempt: false,
  onSubmit: () => {},
  onCancel: () => {},
  ...overrides,
})

const repoProps = (overrides: Partial<DialogProps> = {}): DialogProps => ({
  container: 'resticRepo',
  name: 'm3-restic',
  wrongAttempt: false,
  onSubmit: () => {},
  onCancel: () => {},
  ...overrides,
})

describe('UnlockDialog, archive: first prompt', () => {
  it('renders the first-prompt copy, names the archive, and focuses the field', async () => {
    mount(UnlockDialog, { target, props: archiveProps() })
    await settle()

    expect(target.querySelector('#unlock-title')?.textContent).toBe('Password needed')
    expect(target.querySelector('#unlock-message')?.textContent).toContain('photos.zip')
    expect(target.querySelector('#unlock-message')?.textContent).toContain('password-protected')

    const input = passwordInput(target)
    expect(document.activeElement).toBe(input)
    expect(input.value).toBe('')
  })

  it('offers no Keychain checkbox, since an archive keeps nothing past this session', async () => {
    mount(UnlockDialog, { target, props: archiveProps() })
    await settle()

    expect(rememberBox(target)).toBeNull()
  })

  it('submits the typed password on Unlock click', async () => {
    const onSubmit = vi.fn()
    mount(UnlockDialog, { target, props: archiveProps({ onSubmit }) })
    await settle()

    await type(target, 'hunter2')
    findButton(target, 'Unlock').click()
    expect(onSubmit).toHaveBeenCalledWith({ password: 'hunter2', remember: false })
  })

  it('submits on Enter in the field', async () => {
    const onSubmit = vi.fn()
    mount(UnlockDialog, { target, props: archiveProps({ onSubmit }) })
    await settle()

    await type(target, 'sesame')
    passwordInput(target).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(onSubmit).toHaveBeenCalledWith({ password: 'sesame', remember: false })
  })

  it('disables Unlock (no submit) while the field is empty', async () => {
    const onSubmit = vi.fn()
    mount(UnlockDialog, { target, props: archiveProps({ onSubmit }) })
    await settle()

    expect(findButton(target, 'Unlock').disabled).toBe(true)
    // Enter with an empty field must not submit either.
    passwordInput(target).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('cancels on the Cancel button', async () => {
    const onCancel = vi.fn()
    mount(UnlockDialog, { target, props: archiveProps({ onCancel }) })
    await settle()

    findButton(target, 'Cancel').click()
    expect(onCancel).toHaveBeenCalledOnce()
  })
})

describe('UnlockDialog, archive: wrong-attempt re-prompt', () => {
  it('shows distinct copy and starts with an empty field', async () => {
    mount(UnlockDialog, { target, props: archiveProps({ wrongAttempt: true }) })
    await settle()

    expect(target.querySelector('#unlock-title')?.textContent).toBe('That didn’t work')
    const message = target.querySelector('#unlock-message')?.textContent ?? ''
    expect(message).toContain('didn’t unlock')
    expect(message).toContain('photos.zip')

    expect(passwordInput(target).value).toBe('')
  })
})

describe('UnlockDialog, restic repo', () => {
  it('names the repo folder and speaks of a backup, not an archive', async () => {
    mount(UnlockDialog, { target, props: repoProps() })
    await settle()

    const message = target.querySelector('#unlock-message')?.textContent ?? ''
    expect(message).toContain('m3-restic')
    expect(message).toContain('backup')
    expect(message).not.toContain('archive')
    expect(document.activeElement).toBe(passwordInput(target))
  })

  it('offers "Remember in Keychain", on by default, and submits it', async () => {
    const onSubmit = vi.fn()
    mount(UnlockDialog, { target, props: repoProps({ onSubmit }) })
    await settle()

    const box = rememberBox(target)
    expect(box?.checked).toBe(true)
    expect(box?.closest('label')?.textContent).toContain('Remember in Keychain')

    await type(target, 'hunter2')
    findButton(target, 'Unlock').click()
    expect(onSubmit).toHaveBeenCalledWith({ password: 'hunter2', remember: true })
  })

  it('submits remember: false once the box is unchecked', async () => {
    const onSubmit = vi.fn()
    mount(UnlockDialog, { target, props: repoProps({ onSubmit }) })
    await settle()

    rememberBox(target)?.click()
    await tick()
    await type(target, 'hunter2')
    findButton(target, 'Unlock').click()
    expect(onSubmit).toHaveBeenCalledWith({ password: 'hunter2', remember: false })
  })

  it('while checking: the field and Unlock are disabled, a status says so, and Cancel still works', async () => {
    const onSubmit = vi.fn()
    const onCancel = vi.fn()
    mount(UnlockDialog, { target, props: repoProps({ onSubmit, onCancel, checking: true }) })
    await settle()

    expect(passwordInput(target).disabled).toBe(true)
    expect(findButton(target, 'Unlock').disabled).toBe(true)
    expect(target.querySelector('[role="status"]')?.getAttribute('aria-label')).toContain('Checking')

    passwordInput(target).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(onSubmit).not.toHaveBeenCalled()

    const cancel = findButton(target, 'Cancel')
    expect(cancel.disabled).toBe(false)
    cancel.click()
    expect(onCancel).toHaveBeenCalledOnce()
  })

  it('clears the field when the repo refuses the password, and keeps it when the repo was unreachable', async () => {
    const props = $state(repoProps())
    mount(UnlockDialog, { target, props })
    await settle()
    await type(target, 'wrong-one')

    // The check runs, then the repo answers "unreachable": the password may be right.
    props.checking = true
    await tick()
    props.checking = false
    props.unreachable = true
    await settle()
    expect(passwordInput(target).value).toBe('wrong-one')
    expect(target.querySelector('#unlock-unreachable')?.textContent).toContain('try again')

    // The next check runs and the repo refuses it: the field empties for a new try.
    props.checking = true
    props.unreachable = false
    await tick()
    props.checking = false
    props.wrongAttempt = true
    props.rejections = 1
    await settle()
    expect(passwordInput(target).value).toBe('')
    expect(target.querySelector('#unlock-title')?.textContent).toBe('That didn’t work')
    expect(target.querySelector('#unlock-unreachable')).toBeNull()
    expect(document.activeElement).toBe(passwordInput(target))
  })
})
