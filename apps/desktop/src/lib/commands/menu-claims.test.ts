import { describe, it, expect, vi, afterEach } from 'vitest'
import { claimMenuCommand, runMenuClaim } from './menu-claims'

describe('menu claims', () => {
  const releases: (() => void)[] = []
  afterEach(() => {
    for (const release of releases.splice(0)) release()
  })

  it('runs the claim while it holds, and stops once released', () => {
    const run = vi.fn()
    const release = claimMenuCommand('file.rename', run)
    expect(runMenuClaim('file.rename')).toBe(true)
    expect(run).toHaveBeenCalledOnce()

    release()
    expect(runMenuClaim('file.rename')).toBe(false)
    expect(run).toHaveBeenCalledOnce()
  })

  it('leaves an unclaimed command alone', () => {
    releases.push(claimMenuCommand('file.rename', vi.fn()))
    expect(runMenuClaim('file.copy')).toBe(false)
  })

  it('the newest claim wins, and releasing a superseded one keeps it', () => {
    const older = vi.fn()
    const newer = vi.fn()
    const releaseOlder = claimMenuCommand('file.rename', older)
    releases.push(claimMenuCommand('file.rename', newer))

    releaseOlder()
    expect(runMenuClaim('file.rename')).toBe(true)
    expect(newer).toHaveBeenCalledOnce()
    expect(older).not.toHaveBeenCalled()
  })

  it('a claim that declines lets the command run as if unclaimed (⌘C in a text field)', () => {
    let inField = true
    const run = vi.fn(() => !inField)
    releases.push(claimMenuCommand('edit.copy', run))
    expect(runMenuClaim('edit.copy')).toBe(false)
    inField = false
    expect(runMenuClaim('edit.copy')).toBe(true)
    expect(run).toHaveBeenCalledTimes(2)
  })
})
