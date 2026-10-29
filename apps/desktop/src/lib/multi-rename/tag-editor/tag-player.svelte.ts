/**
 * The Tags window's "Open in <app>" menu: the system's default app for the tracks, the other apps that can open them,
 * and "Choose…" (the system's open panel). A pick is remembered in `multiRename.tagEditor.player` (`'default'` or a
 * bundle id) and opens the tracks at once. The names come from the backend (`tag_player_choices`), asked when the
 * window opens and again when the menu does, so an app installed meanwhile shows.
 */
import type { AppChoice } from '#lib/ipc/bindings.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { createMenu, type MenuController } from '#lib/ui/menu-controller.svelte.js'
import type { MenuItem } from '#lib/ui/menu-types.js'

const log = getAppLogger('multiRename')

const DEFAULT_VALUE = 'default'
const CHOOSE_VALUE = 'choose'
const NO_OTHER_VALUE = 'none'

type Answer<T> = { ok: true; value: T } | { ok: false; error: unknown }

export interface TagPlayerDeps {
  /** The apps for the session's tracks, plus `chosen`'s app when it's installed. */
  choices: (chosen: string) => Promise<Answer<AppChoice[]>>
  /** The system's open panel; `null` when the user cancels. */
  choose: () => Promise<Answer<AppChoice | null>>
  getPlayer: () => string
  setPlayer: (player: string) => void
  /** Opens the tracks per the remembered player. */
  open: () => Promise<void>
}

export interface TagPlayer {
  readonly menu: MenuController<AppChoice>
  /** The name the button and the ⇧F3 key show ("Open in <name>"), or `null` while unknown. */
  readonly appName: string | null
  load: () => Promise<void>
  /** Opens the menu under `anchor`, after asking again which apps there are. */
  openMenu: (anchor: HTMLElement) => Promise<void>
  /** A menu row's pick: remembers it and opens. */
  pick: (value: string) => Promise<void>
}

export function createTagPlayer(deps: TagPlayerDeps): TagPlayer {
  let apps = $state.raw<AppChoice[]>([])
  let player = $state(deps.getPlayer())

  const defaultApp = $derived(apps.find((app) => app.isDefault))
  const chosenApp = $derived(player === DEFAULT_VALUE ? undefined : apps.find((app) => app.bundleId === player))
  // A chosen app that's gone launches the default app, so that's the name to show.
  const shown = $derived(chosenApp ?? defaultApp)

  async function load(): Promise<void> {
    try {
      const res = await deps.choices(deps.getPlayer())
      if (res.ok) apps = res.value
    } catch (e) {
      // No list is a button that says "Open in player" and a menu with only Choose…, never a broken window.
      log.warn('Listing the player apps failed: {reason}', { reason: e instanceof Error ? e.name : typeof e })
    }
  }

  function remember(value: string): void {
    deps.setPlayer(value)
    player = value
  }

  async function pick(value: string): Promise<void> {
    if (value === CHOOSE_VALUE) {
      const res = await deps.choose()
      if (!res.ok || res.value === null) return
      const picked = res.value
      if (!apps.some((app) => app.bundleId === picked.bundleId)) apps = [...apps, { ...picked, isDefault: false }]
      remember(picked.bundleId)
    } else {
      remember(value)
    }
    await deps.open()
  }

  function rows(): MenuItem<AppChoice>[] {
    const using = chosenApp?.bundleId ?? DEFAULT_VALUE
    const row = (value: string, label: string, app?: AppChoice): MenuItem<AppChoice> => ({
      value,
      label,
      data: app,
      check: using === value ? { kind: 'current' } : undefined,
    })
    const defaultLabel =
      defaultApp === undefined
        ? tString('multiRename.tags.play.defaultAppUnknown')
        : tString('multiRename.tags.play.defaultApp', { app: defaultApp.name })
    const others = apps.filter((app) => !app.isDefault)
    return [
      row(DEFAULT_VALUE, defaultLabel, defaultApp),
      ...others.map((app) => row(app.bundleId, app.name, app)),
      // Nothing else opens every type of the tracks: say so rather than show a menu that looks unfinished.
      ...(others.length === 0
        ? [{ value: NO_OTHER_VALUE, label: tString('multiRename.tags.play.noOtherApp'), disabled: true }]
        : []),
    ]
  }

  const menu = createMenu<AppChoice>({
    getSections: () => [
      { id: 'apps', items: rows() },
      { id: 'choose', items: [{ value: CHOOSE_VALUE, label: tString('multiRename.tags.play.chooseOther') }] },
    ],
    onSelect: (item) => {
      void pick(item.value)
    },
  })

  return {
    menu,
    get appName() {
      return shown?.name ?? null
    },
    load,
    async openMenu(anchor) {
      await load()
      menu.openUnder(anchor)
    },
    pick,
  }
}
