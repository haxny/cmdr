/**
 * The Tags window's player: one `<audio>` element and one `cmdr-media://` URL at a time. A URL is minted per play
 * (`tagTrackAudioUrl`) and released on every way playback ends: another track, the same track again, `stop()`, the
 * track running out, a decode error. A URL that arrives after playback was already stopped is released unused.
 */
import { getAppLogger } from '#lib/logging/logger.js'
import { releaseTagTrackAudio, tagTrackAudioUrl, type TagEditorResult } from '#lib/tauri-commands/index.js'

const log = getAppLogger('multiRename')

export interface TrackPlayerDeps {
  requestUrl?: (args: { sessionId: string; path: string }) => Promise<TagEditorResult<string>>
  releaseUrl?: (url: string) => Promise<void>
  createAudio?: () => HTMLAudioElement
}

export interface TrackPlayer {
  /** The path of the track playing now (or being fetched to play), or `null`. */
  readonly playing: string | null
  /** The last play failed (no URL, a decode error, a refused `play()`); cleared by the next toggle or stop. */
  readonly failed: boolean
  /** Plays `path`, stopping what plays; or stops it when it is the track that plays. */
  toggle(path: string): Promise<void>
  stop(): void
}

export function createTrackPlayer(sessionId: string, deps: TrackPlayerDeps = {}): TrackPlayer {
  const requestUrl = deps.requestUrl ?? (({ sessionId: id, path }) => tagTrackAudioUrl(id, path))
  const releaseUrl = deps.releaseUrl ?? releaseTagTrackAudio
  const createAudio = deps.createAudio ?? (() => new Audio())

  let playing = $state<string | null>(null)
  let failed = $state(false)
  let audio: HTMLAudioElement | null = null
  let url: string | null = null
  /** Counts plays and stops; an answer for an older one than the latest is stale. */
  let generation = 0

  function release(target: string): void {
    releaseUrl(target).catch((e: unknown) => {
      log.warn("couldn't release a track's audio URL: {error}", { error: e })
    })
  }

  /** Ends what plays and invalidates any URL still on its way. */
  function end(): void {
    generation += 1
    const element = audio
    const target = url
    audio = null
    url = null
    playing = null
    if (element !== null) {
      element.pause()
      element.removeAttribute('src')
      element.load()
    }
    if (target !== null) release(target)
  }

  function fail(mine: number): void {
    if (mine !== generation) return
    end()
    failed = true
  }

  async function play(path: string, mine: number): Promise<void> {
    let res: TagEditorResult<string>
    try {
      res = await requestUrl({ sessionId, path })
    } catch (e) {
      log.warn("couldn't get a track's audio URL: {error}", { error: e })
      fail(mine)
      return
    }
    if (mine !== generation) {
      if (res.ok) release(res.value)
      return
    }
    if (!res.ok) {
      log.warn("couldn't get a track's audio URL: {kind}", { kind: res.error.kind })
      fail(mine)
      return
    }
    const element = createAudio()
    element.addEventListener('ended', () => {
      if (mine === generation) end()
    })
    element.addEventListener('error', () => {
      fail(mine)
    })
    audio = element
    url = res.value
    element.src = res.value
    try {
      await element.play()
    } catch (e) {
      // A pause during loading rejects too; only a play nobody stopped is a failure.
      if (mine === generation) log.warn("a track didn't start: {error}", { error: e })
      fail(mine)
    }
  }

  return {
    get playing() {
      return playing
    },
    get failed() {
      return failed
    },
    async toggle(path) {
      const same = playing === path
      end()
      failed = false
      if (same) return
      playing = path
      await play(path, generation)
    },
    stop() {
      end()
      failed = false
    },
  }
}
