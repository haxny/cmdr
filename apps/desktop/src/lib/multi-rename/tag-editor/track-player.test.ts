/** The track player: one audio element, one URL at a time, released on every way playback ends. */
import { describe, expect, it, vi } from 'vitest'
import { createTrackPlayer } from './track-player.svelte'

let refusePlay = false

class FakeAudio {
  src = ''
  paused = true
  private listeners = new Map<string, Set<() => void>>()
  play = vi.fn(() => {
    this.paused = false
    if (!refusePlay) return Promise.resolve()
    refusePlay = false
    return Promise.reject(new Error('NotAllowed'))
  })
  pause = vi.fn(() => {
    this.paused = true
  })
  removeAttribute = vi.fn((name: string) => {
    if (name === 'src') this.src = ''
  })
  load = vi.fn()
  addEventListener(type: string, fn: () => void): void {
    const set = this.listeners.get(type) ?? new Set()
    set.add(fn)
    this.listeners.set(type, set)
  }
  removeEventListener(type: string, fn: () => void): void {
    this.listeners.get(type)?.delete(fn)
  }
  emit(type: string): void {
    for (const fn of this.listeners.get(type) ?? []) fn()
  }
}

function setup(over: { requestUrl?: (args: { sessionId: string; path: string }) => Promise<unknown> } = {}) {
  const audios: FakeAudio[] = []
  let n = 0
  const requestUrl = vi.fn(
    over.requestUrl ??
      (({ path }: { sessionId: string; path: string }) =>
        Promise.resolve({ ok: true, value: `cmdr-media://localhost/${path}-${String(++n)}` })),
  )
  const releaseUrl = vi.fn(() => Promise.resolve())
  const player = createTrackPlayer('s1', {
    requestUrl: requestUrl as never,
    releaseUrl,
    createAudio: () => {
      const audio = new FakeAudio()
      audios.push(audio)
      return audio as unknown as HTMLAudioElement
    },
  })
  return { player, audios, requestUrl, releaseUrl }
}

describe('createTrackPlayer', () => {
  it('plays a track from the URL the session minted', async () => {
    const { player, audios, requestUrl } = setup()
    await player.toggle('/b/a.mp3')
    expect(requestUrl).toHaveBeenCalledWith({ sessionId: 's1', path: '/b/a.mp3' })
    expect(player.playing).toBe('/b/a.mp3')
    expect(audios).toHaveLength(1)
    expect(audios[0].src).toBe('cmdr-media://localhost//b/a.mp3-1')
    expect(audios[0].play).toHaveBeenCalledOnce()
  })

  it('playing B stops A and releases its URL', async () => {
    const { player, audios, releaseUrl } = setup()
    await player.toggle('/b/a.mp3')
    await player.toggle('/b/b.mp3')
    expect(audios[0].pause).toHaveBeenCalled()
    expect(releaseUrl).toHaveBeenCalledWith('cmdr-media://localhost//b/a.mp3-1')
    expect(player.playing).toBe('/b/b.mp3')
    expect(releaseUrl).toHaveBeenCalledTimes(1)
  })

  it('toggling the playing track stops it and releases the URL', async () => {
    const { player, audios, releaseUrl } = setup()
    await player.toggle('/b/a.mp3')
    await player.toggle('/b/a.mp3')
    expect(player.playing).toBeNull()
    expect(audios[0].pause).toHaveBeenCalled()
    expect(releaseUrl).toHaveBeenCalledWith('cmdr-media://localhost//b/a.mp3-1')
  })

  it('stop() ends playback and releases the URL; with nothing playing it does nothing', async () => {
    const { player, releaseUrl } = setup()
    player.stop()
    expect(releaseUrl).not.toHaveBeenCalled()
    await player.toggle('/b/a.mp3')
    player.stop()
    expect(player.playing).toBeNull()
    expect(releaseUrl).toHaveBeenCalledTimes(1)
  })

  it('an ended event clears playing and releases the URL', async () => {
    const { player, audios, releaseUrl } = setup()
    await player.toggle('/b/a.mp3')
    audios[0].emit('ended')
    expect(player.playing).toBeNull()
    expect(releaseUrl).toHaveBeenCalledWith('cmdr-media://localhost//b/a.mp3-1')
  })

  it('a failed URL request plays nothing and says so', async () => {
    const { player, audios, releaseUrl } = setup({
      requestUrl: () => Promise.resolve({ ok: false, error: { kind: 'unknownTrack' } }),
    })
    await player.toggle('/b/a.mp3')
    expect(player.playing).toBeNull()
    expect(player.failed).toBe(true)
    expect(audios).toHaveLength(0)
    expect(releaseUrl).not.toHaveBeenCalled()
    await player.toggle('/b/a.mp3')
    expect(player.failed).toBe(true)
  })

  it('a decode error clears playing, releases the URL and says so', async () => {
    const { player, audios, releaseUrl } = setup()
    await player.toggle('/b/a.mp3')
    audios[0].emit('error')
    expect(player.playing).toBeNull()
    expect(player.failed).toBe(true)
    expect(releaseUrl).toHaveBeenCalledOnce()
  })

  it('a refused play() releases the URL and says so', async () => {
    const { player, audios, releaseUrl } = setup()
    refusePlay = true
    await player.toggle('/b/a.mp3')
    expect(audios).toHaveLength(1)
    expect(player.playing).toBeNull()
    expect(player.failed).toBe(true)
    expect(releaseUrl).toHaveBeenCalledOnce()
  })

  it('a URL that arrives after stop() is released and never plays', async () => {
    let resolve: (value: unknown) => void = () => {}
    const { player, audios, releaseUrl } = setup({
      requestUrl: () =>
        new Promise((r) => {
          resolve = r
        }),
    })
    const pending = player.toggle('/b/a.mp3')
    expect(player.playing).toBe('/b/a.mp3')
    player.stop()
    resolve({ ok: true, value: 'cmdr-media://localhost/late' })
    await pending
    expect(audios).toHaveLength(0)
    expect(releaseUrl).toHaveBeenCalledWith('cmdr-media://localhost/late')
    expect(player.playing).toBeNull()
  })

  it('a new play clears an earlier failure', async () => {
    let fail = true
    const { player } = setup({
      requestUrl: () =>
        Promise.resolve(
          fail ? { ok: false, error: { kind: 'unknownTrack' } } : { ok: true, value: 'cmdr-media://localhost/x' },
        ),
    })
    await player.toggle('/b/a.mp3')
    expect(player.failed).toBe(true)
    fail = false
    await player.toggle('/b/b.mp3')
    expect(player.failed).toBe(false)
    expect(player.playing).toBe('/b/b.mp3')
  })
})
