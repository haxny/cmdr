/** The Tags window's "Open in <app>" menu: the default app, the other candidates, Choose…; the choice is remembered. */
import { describe, expect, it, vi } from 'vitest'
import type { AppChoice } from '#lib/ipc/bindings.js'
import { createTagPlayer } from './tag-player.svelte'

const music: AppChoice = { bundleId: 'com.apple.Music', name: 'Music', isDefault: true }
const vlc: AppChoice = { bundleId: 'org.videolan.vlc', name: 'VLC', isDefault: false }

function setup(over: { player?: string; apps?: AppChoice[]; picked?: AppChoice | null } = {}) {
  let player = over.player ?? 'default'
  const apps = over.apps ?? [music, vlc]
  const open = vi.fn(() => Promise.resolve())
  const setPlayer = vi.fn((value: string) => {
    player = value
  })
  const choose = vi.fn(() => Promise.resolve({ ok: true as const, value: over.picked ?? null }))
  const tagPlayer = createTagPlayer({
    choices: (chosen) => Promise.resolve({ ok: true as const, value: chosen === 'default' ? apps : [...apps] }),
    choose,
    getPlayer: () => player,
    setPlayer,
    open,
  })
  return { tagPlayer, open, setPlayer, choose, player: () => player }
}

const items = (t: ReturnType<typeof setup>['tagPlayer']) => t.menu.sections.flatMap((s) => s.items)

describe('createTagPlayer', () => {
  it('names the default app on the button until the user picks another', async () => {
    const { tagPlayer } = setup()
    expect(tagPlayer.appName).toBeNull()
    await tagPlayer.load()
    expect(tagPlayer.appName).toBe('Music')
  })

  it('names the chosen app, and the default when the chosen one is gone', async () => {
    const chosen = setup({ player: 'org.videolan.vlc' })
    await chosen.tagPlayer.load()
    expect(chosen.tagPlayer.appName).toBe('VLC')
    const gone = setup({ player: 'gone.app' })
    await gone.tagPlayer.load()
    expect(gone.tagPlayer.appName).toBe('Music')
  })

  it('has no name when nothing can open the tracks', async () => {
    const { tagPlayer } = setup({ apps: [] })
    await tagPlayer.load()
    expect(tagPlayer.appName).toBeNull()
  })

  it('lists the default app, the others, then Choose…, with a check on the one in use', async () => {
    const { tagPlayer } = setup({ player: 'org.videolan.vlc' })
    await tagPlayer.load()
    expect(items(tagPlayer).map((i) => i.value)).toEqual(['default', 'org.videolan.vlc', 'choose'])
    expect(items(tagPlayer).find((i) => i.check)?.value).toBe('org.videolan.vlc')
  })

  it('picking the default app remembers it and opens', async () => {
    const { tagPlayer, setPlayer, open } = setup({ player: 'org.videolan.vlc' })
    await tagPlayer.load()
    await tagPlayer.pick('default')
    expect(setPlayer).toHaveBeenCalledWith('default')
    expect(open).toHaveBeenCalledWith()
    expect(tagPlayer.appName).toBe('Music')
  })

  it('picking another app remembers its bundle id and opens', async () => {
    const { tagPlayer, setPlayer, open } = setup()
    await tagPlayer.load()
    await tagPlayer.pick('org.videolan.vlc')
    expect(setPlayer).toHaveBeenCalledWith('org.videolan.vlc')
    expect(open).toHaveBeenCalledOnce()
    expect(tagPlayer.appName).toBe('VLC')
  })

  it('Choose… remembers the app the picker gave and opens in it', async () => {
    const foobar: AppChoice = { bundleId: 'com.foobar2000.mac', name: 'foobar2000', isDefault: false }
    const { tagPlayer, setPlayer, open } = setup({ picked: foobar })
    await tagPlayer.load()
    await tagPlayer.pick('choose')
    expect(setPlayer).toHaveBeenCalledWith('com.foobar2000.mac')
    expect(open).toHaveBeenCalledOnce()
    expect(tagPlayer.appName).toBe('foobar2000')
    expect(items(tagPlayer).map((i) => i.value)).toContain('com.foobar2000.mac')
  })

  it('a cancelled picker changes nothing and opens nothing', async () => {
    const { tagPlayer, setPlayer, open } = setup({ picked: null })
    await tagPlayer.load()
    await tagPlayer.pick('choose')
    expect(setPlayer).not.toHaveBeenCalled()
    expect(open).not.toHaveBeenCalled()
  })
})
