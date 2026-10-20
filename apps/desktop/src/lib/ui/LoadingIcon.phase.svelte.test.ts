/**
 * `LoadingIcon` during a listing PHASE: work a listing does before its first
 * entry, with its own honest measure. Today that's a restic backup's index load,
 * which reports the index files' bytes read so far and in all.
 */
import { describe, it, expect, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import LoadingIcon from './LoadingIcon.svelte'
import type { ListingPhase } from '#lib/ipc/bindings.js'

const MB = 1_000_000

function indexLoad(doneBytes: number, totalBytes = 312 * MB): ListingPhase {
  return { kind: 'loadingIndex', doneBytes, totalBytes, doneFiles: 1, totalFiles: 40 }
}

const text = (target: HTMLElement) =>
  (target.querySelector('.loading-text')?.textContent ?? '').replace(/\s+/g, ' ').trim()
const bar = (target: HTMLElement) => target.querySelector('[role="progressbar"]')

let cleanup: (() => void) | undefined

afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

/** Mounts with live props, so a test can script the sequence a real load sends. */
function mountLive(initial: { phase?: ListingPhase | null; loadedCount?: number; finalizingCount?: number }) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const props = $state({ showCancelHint: true, ...initial })
  const instance = mount(LoadingIcon, { target, props })
  flushSync()
  cleanup = () => {
    void unmount(instance)
    target.remove()
  }
  return { target, props }
}

describe('LoadingIcon: a backup index load', () => {
  it('says how much of the index has loaded, in bytes, with a bar', () => {
    const { target } = mountLive({ phase: indexLoad(48 * MB), loadedCount: 0 })

    expect(text(target)).toBe('Loading the backup index: 48 MB of 312 MB')
    expect(bar(target)?.getAttribute('aria-valuenow')).toBe('15')
    expect(bar(target)?.getAttribute('aria-valuetext')).toBe('48 MB of 312 MB')
    expect(bar(target)?.getAttribute('aria-label')).toBe('Backup index')
  })

  it('follows a scripted load from start to the first entries', () => {
    const { target, props } = mountLive({ phase: indexLoad(0), loadedCount: 0 })
    expect(text(target)).toBe('Loading the backup index: 0 bytes of 312 MB')
    expect(bar(target)?.getAttribute('aria-valuenow')).toBe('0')

    props.phase = indexLoad(156 * MB)
    flushSync()
    expect(bar(target)?.getAttribute('aria-valuenow')).toBe('50')

    props.phase = indexLoad(312 * MB)
    flushSync()
    expect(bar(target)?.getAttribute('aria-valuenow')).toBe('100')

    // Entries start arriving: the phase is over, so the ordinary count takes over.
    props.phase = null
    props.loadedCount = 1200
    flushSync()
    expect(text(target)).toBe('Loaded 1,200 files…')
    expect(bar(target)).toBeNull()
  })

  it('shows an empty bar rather than nonsense for an index with no bytes to read', () => {
    const { target } = mountLive({ phase: indexLoad(0, 0) })
    expect(bar(target)?.getAttribute('aria-valuenow')).toBe('0')
  })

  it('gives way to the finalizing message', () => {
    const { target } = mountLive({ phase: indexLoad(312 * MB), finalizingCount: 30 })
    expect(text(target)).toBe('All 30 files loaded. Sorting your files, preparing view…')
    expect(bar(target)).toBeNull()
  })

  it('keeps the Esc hint, since the load stays cancelable', () => {
    const { target } = mountLive({ phase: indexLoad(48 * MB) })
    expect(target.querySelector('.cancel-hint')).not.toBeNull()
  })
})
