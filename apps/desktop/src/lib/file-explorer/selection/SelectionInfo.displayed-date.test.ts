/**
 * Brief's status bar shows the date the pane's rows are sorted by, the same one Full's
 * date column would (`displayedDateColumn`), and its tooltip marks that date as the
 * displayed one.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount, type ComponentProps } from 'svelte'
import SelectionInfo from './SelectionInfo.svelte'
import type { FileEntry, SortColumn } from '../types'

vi.mock('#lib/indexing/index-state.svelte.js', () => ({
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
  getWalkedGround: () => [],
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatDateTime: (t: number | null | undefined) => (t != null ? String(t) : ''),
  // The timestamp itself, so the line says which date it shows.
  formattedDate: (t: number | null | undefined) =>
    t != null ? { text: String(t), segments: [{ text: String(t), ageClass: null }] } : { text: '', segments: [] },
  getSizeDisplayMode: () => 'smart',
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
}))

const MODIFIED = 1_000
const CREATED = 1_500
const ADDED = 2_000

const file: FileEntry = {
  name: 'a.txt',
  path: '/dir/a.txt',
  isDirectory: false,
  isSymlink: false,
  size: 12,
  modifiedAt: MODIFIED,
  createdAt: CREATED,
  addedAt: ADDED,
  permissions: 0o644,
  owner: 'test',
  group: 'staff',
  iconId: 'file',
}

let mounted: ReturnType<typeof mount> | undefined

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = undefined
  document.body.innerHTML = ''
})

async function render(effectiveSortBy: SortColumn | null, entry: FileEntry = file): Promise<HTMLElement> {
  if (mounted) void unmount(mounted)
  const target = document.createElement('div')
  document.body.appendChild(target)
  const props: ComponentProps<typeof SelectionInfo> = {
    viewMode: 'brief',
    volumeId: 'root',
    entry,
    effectiveSortBy,
    stats: null,
    selectedCount: 0,
  }
  mounted = mount(SelectionInfo, { target, props })
  await tick()
  return target
}

function dateText(target: HTMLElement): string {
  return target.querySelector('.date')?.textContent.trim() ?? ''
}

async function dateTooltip(target: HTMLElement): Promise<string> {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  try {
    const date = target.querySelector<HTMLElement>('.date')
    date?.dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(500)
    await tick()
    const tip = document.querySelector('.cmdr-tooltip.visible')?.textContent ?? ''
    date?.dispatchEvent(new MouseEvent('mouseleave'))
    return tip
  } finally {
    vi.useRealTimers()
  }
}

describe('SelectionInfo date', () => {
  it('shows the date added under an Added sort, marked displayed in the tooltip', async () => {
    const target = await render('added')

    expect(dateText(target)).toBe(String(ADDED))
    expect(await dateTooltip(target)).toBe(
      `Created: ${String(CREATED)}Date added (displayed): ${String(ADDED)}Last modified: ${String(MODIFIED)}`,
    )
  })

  it('shows the creation date under a Created sort', async () => {
    const target = await render('created')

    expect(dateText(target)).toBe(String(CREATED))
  })

  // The backend reports the column the rows really follow, so an Added sort the location
  // can't honor arrives here as Modified.
  it('shows the modified date when an Added sort fell back to Modified', async () => {
    const target = await render('modified')

    expect(dateText(target)).toBe(String(MODIFIED))
    expect(await dateTooltip(target)).toContain(`Last modified (displayed): ${String(MODIFIED)}`)
  })

  it('shows the modified date under a Name sort and in a ranked search pane', async () => {
    expect(dateText(await render('name'))).toBe(String(MODIFIED))
    expect(dateText(await render(null))).toBe(String(MODIFIED))
  })

  it('shows no date for an entry that lacks the displayed one', async () => {
    const target = await render('added', { ...file, addedAt: undefined })

    expect(dateText(target)).toBe('')
  })
})
