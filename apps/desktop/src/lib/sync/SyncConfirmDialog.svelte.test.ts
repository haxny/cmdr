/**
 * The confirm over a real run state and a fake IPC: older-replaces-newer rows come first with both dates, the
 * versions folder is named where replaced files go, Enter accepts only a run it may, and unticking a group leaves it
 * out of the run.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import SyncConfirmDialog from './SyncConfirmDialog.svelte'
import { createSyncRun } from './sync-run.svelte'
import { fakeSyncIpc, preview, settle } from './sync-test-fixtures'
import type { ConfirmPreview } from '#lib/tauri-commands/sync.js'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const DEFAULTS = {
  keep: { place: 'versionsAlways' as const, location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
  timeToleranceSecs: 2,
  ignoreHourShift: false,
  guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
}

let component: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (component) void unmount(component)
  component = null
  document.body.innerHTML = ''
})

async function mountConfirm(shown: ConfirmPreview) {
  const fake = fakeSyncIpc()
  vi.mocked(fake.ipc.getSyncConfirm).mockResolvedValue(shown)
  const session = { sessionId: 's1', filters: { ...DEFAULT_FILTERS }, compare: vi.fn(), refresh: vi.fn() }
  const run = createSyncRun({ ipc: fake.ipc, session, readDefaults: () => DEFAULTS })
  await run.openConfirm()
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(SyncConfirmDialog, { target, props: { run } })
  await settle()
  await tick()
  return { fake, run, doc: target.ownerDocument }
}

const DEFAULT_FILTERS = {
  toRight: true,
  equal: false,
  different: true,
  toLeft: true,
  duplicates: true,
  singles: true,
  unknown: true,
  warningsOnly: false,
}

const MIRROR = preview({
  groups: [
    { key: 'copyRight', files: 2, bytes: 2048, keep: null },
    {
      key: 'replaceRight',
      files: 1,
      bytes: 10,
      keep: { type: 'folder', volumeId: 'smb-1', path: '/Volumes/NAS/Photos/.cmdr-sync/versions/2026-11-01 10.00.00' },
    },
  ],
  olderOverwritesNewer: [{ item: 3, relPath: '2024/beach.jpg', leftMtime: 1_700_000_000, rightMtime: 1_780_000_000 }],
  olderOverwritesNewerTotal: 1,
})

function press(doc: Document, init: KeyboardEventInit): void {
  const target = doc.querySelector<HTMLElement>('[role="alertdialog"], [role="dialog"]') ?? doc.body
  target.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
}

describe('SyncConfirmDialog', () => {
  it('lists older-replaces-newer first with both dates', async () => {
    const { doc } = await mountConfirm(MIRROR)
    const dialog = doc.querySelector('[role="alertdialog"]')
    expect(dialog).not.toBeNull()
    const older = doc.querySelector('.older')
    const groups = doc.querySelector('.groups')
    expect(older?.textContent).toContain('Older replaces newer: 1 file')
    expect(older?.compareDocumentPosition(groups as Node)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
    const row = older?.querySelector('li')?.textContent ?? ''
    expect(row).toMatch(/^2024\/beach\.jpg: left .+, right .+$/)
    expect(row).not.toContain('no date')
  })

  it('names the versions folder path when the target has no Trash', async () => {
    const { doc } = await mountConfirm(MIRROR)
    expect(doc.body.textContent).toContain(
      'Replaced and removed files go to the versions folder /Volumes/NAS/Photos/.cmdr-sync/versions/2026-11-01 10.00.00.',
    )
  })

  it('Enter refuses a run that replaces newer files; the Synchronize button runs it', async () => {
    const { doc, fake } = await mountConfirm(MIRROR)
    press(doc, { key: 'Enter' })
    await settle()
    expect(fake.ipc.startSyncRun).not.toHaveBeenCalled()
    expect(doc.querySelector('.status')?.textContent).toBe(
      'Press Synchronize to run this: it removes files, replaces newer ones, or needs a second yes.',
    )
    const run = [...doc.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Synchronize')
    run?.click()
    await settle()
    expect(fake.ipc.startSyncRun).toHaveBeenCalledWith('s1', {
      revision: 7,
      groups: ['copyRight', 'replaceRight'],
      acknowledged: [],
      defaults: DEFAULTS,
    })
  })

  it('Enter accepts a copy-only run, without the groups left unticked', async () => {
    const shown = preview({
      groups: [
        { key: 'copyRight', files: 2, bytes: 20, keep: null },
        { key: 'copyLeft', files: 1, bytes: 10, keep: null },
      ],
    })
    const { doc, fake } = await mountConfirm(shown)
    expect(doc.querySelector('[role="dialog"]')).not.toBeNull()
    const boxes = [...doc.querySelectorAll<HTMLElement>('.groups [role="checkbox"], .groups input[type="checkbox"]')]
    boxes.at(-1)?.click()
    await settle()
    await tick()
    press(doc, { key: 'Enter' })
    await settle()
    expect(vi.mocked(fake.ipc.startSyncRun).mock.calls[0]?.[1].groups).toEqual(['copyRight'])
  })

  it('a guard says what it holds and offers its second yes', async () => {
    const shown = preview({
      groups: [{ key: 'removeRight', files: 61, bytes: 61, keep: { type: 'trash' } }],
      blocks: [{ type: 'maxRemove', side: 'right', removals: 61, targetFiles: 100 }],
    })
    const { doc } = await mountConfirm(shown)
    expect(doc.body.textContent).toContain(
      'This removes 61 of 100 files on the right, more than the limit in Settings allows. Those removals wait for Remove anyway.',
    )
    expect(doc.body.textContent).toContain('Remove anyway')
    expect(doc.body.textContent).toContain('Replaced and removed files go to the Trash.')
  })

  it('names the `?` rows you gave an action, with both dates', async () => {
    const shown = preview({
      groups: [{ key: 'copyRight', files: 1, bytes: 20, keep: null }],
      chosenUnknown: [{ item: 2, relPath: 'notes/draft.txt', leftMtime: null, rightMtime: null }],
      chosenUnknownTotal: 1,
    })
    const { doc } = await mountConfirm(shown)
    const unknown = doc.querySelector('.unknown')
    expect(unknown?.textContent).toContain('1 row marked ?')
    expect(unknown?.querySelector('li')?.textContent).toBe('notes/draft.txt: left no date, right no date')
  })

  it('while a ticked group waits for its second yes, Synchronize is off and says how many files wait', async () => {
    const shown = preview({
      groups: [
        { key: 'copyRight', files: 2, bytes: 20, keep: null },
        { key: 'removeRight', files: 61, bytes: 61, keep: { type: 'trash' } },
      ],
      blocks: [{ type: 'maxRemove', side: 'right', removals: 61, targetFiles: 100 }],
    })
    const { doc, fake } = await mountConfirm(shown)
    const button = [...doc.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Synchronize')
    expect(button?.getAttribute('aria-disabled')).toBe('true')
    expect(doc.querySelector('.status')?.textContent).toBe(
      '61 files wait for Remove anyway. Tick it, or untick that group.',
    )
    button?.click()
    await settle()
    expect(fake.ipc.startSyncRun).not.toHaveBeenCalled()
    const ack = [...doc.querySelectorAll<HTMLElement>('.blocks [role="checkbox"], .blocks input[type="checkbox"]')]
    ack[0]?.click()
    await settle()
    await tick()
    expect(button?.getAttribute('aria-disabled')).toBeNull()
  })

  it('a group no button releases is shown held, with why, and Synchronize says nothing can run', async () => {
    const shown = preview({
      groups: [{ key: 'copyLeft', files: 3, bytes: 30, keep: null }],
      blocks: [{ type: 'sentinelMissing', side: 'right' }],
    })
    const { doc, run } = await mountConfirm(shown)
    expect(run.accepted).toEqual([])
    const box = doc.querySelector<HTMLInputElement>('.groups input[type="checkbox"]')
    expect(box?.disabled).toBe(true)
    const reason = doc.getElementById(box?.getAttribute('aria-describedby')?.split(' ').at(-1) ?? '')
    expect(reason?.textContent).toContain('The check file Settings names is missing on the right')
    const button = [...doc.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Synchronize')
    expect(button?.getAttribute('aria-disabled')).toBe('true')
    expect(doc.querySelector('.status')?.textContent).toBe('Nothing here can run: a guard holds every group.')
  })

  it('opens with focus on the first group', async () => {
    const { doc } = await mountConfirm(MIRROR)
    await new Promise((resolve) => requestAnimationFrame(resolve))
    const first = doc.querySelector('.groups input[type="checkbox"]')
    expect(doc.activeElement).toBe(first)
  })
})
