/**
 * Behavior tests for `Menu.svelte`, the surface. The controller's own contract (keys,
 * pointer mode, reorder) is pinned in `menu-controller.svelte.test.ts`; this file covers
 * what the component renders and the two rules that belong to the DOM: a row's own controls
 * acting for themselves, and the snippets decorating rather than replacing a row.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick, unmount, createRawSnippet } from 'svelte'
import Menu from './Menu.svelte'
import { createMenu, type MenuController } from './menu-controller.svelte'
import { disclosureRowValue } from './menu-navigation'
import type { MenuRowContext, MenuSection } from './menu-types'
import { assertFocusModel } from '#lib/test-focus-model.js'

function sections(): MenuSection[] {
  return [
    {
      id: 'favorites',
      heading: 'Favorites',
      reorderable: true,
      items: [
        { value: 'projects', label: 'Projects', icon: { lucide: 'folder' } },
        { value: 'downloads', label: 'Downloads' },
      ],
    },
    {
      id: 'volumes',
      heading: 'Volumes',
      items: [
        { value: 'hd', label: 'Macintosh HD', check: { kind: 'current' } },
        { value: 'backup', label: 'Backup', disabled: true },
        {
          value: 'share',
          label: 'Team share',
          submenu: [
            { value: 'connect', label: 'Connect directly' },
            { value: 'forget', label: 'Forget this share' },
            { value: 'fast', label: 'Use the fast connection', check: { kind: 'toggle', checked: true } },
          ],
        },
      ],
    },
    { id: 'empty', heading: 'Nothing here', items: [], emptyLabel: '(This section is empty)' },
  ]
}

let mounted: (() => void)[] = []

/** Mounts the surface around a controller, opened at a point. Returns both. */
async function open(
  props: Record<string, unknown> = {},
  deps: Record<string, unknown> = {},
): Promise<{ menu: MenuController }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const menu = createMenu({ getSections: sections, onSelect: () => {}, ...deps })
  menu.openAt({ x: 50, y: 50 })
  const component = mount(Menu, { target, props: { menu, ariaLabel: 'Volumes', ...props } })
  await tick()
  await tick()
  mounted.push(() => {
    menu.destroy()
    void unmount(component)
  })
  return { menu }
}

function surface(): HTMLElement | null {
  return document.querySelector('[data-menu]')
}

function row(value: string): HTMLElement | null {
  return document.querySelector(`[data-menu-row="${value}"]`)
}

afterEach(() => {
  for (const dispose of mounted) dispose()
  mounted = []
  document.body.innerHTML = ''
})

describe('rendering', () => {
  it('renders nothing while the menu is closed, so a consumer writes no {#if}', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const menu = createMenu({ getSections: sections, onSelect: () => {} })
    const component = mount(Menu, { target, props: { menu, ariaLabel: 'Volumes' } })
    await tick()
    expect(surface()).toBeNull()
    menu.destroy()
    void unmount(component)
  })

  it('renders a labelled menu with one group per section, measured and visible', async () => {
    await open()
    expect(surface()?.getAttribute('aria-label')).toBe('Volumes')
    // ❗ It starts `visibility: hidden` and is only shown once measured. If measuring can
    // miss (the portal mounts a beat later), the menu takes keys and shows nothing.
    expect(surface()?.style.visibility).toBe('visible')
    expect(surface()?.style.top).toBeTruthy()
    expect(document.querySelectorAll('[role="group"]')).toHaveLength(3)
    expect(document.body.textContent).toContain('Favorites')
    expect(document.body.textContent).toContain('Macintosh HD')
  })

  it('renders every row as a menuitem, marking the disabled one', async () => {
    await open()
    expect(row('hd')).not.toBeNull()
    expect(row('backup')?.getAttribute('aria-disabled')).toBe('true')
    expect(row('projects')?.getAttribute('aria-disabled')).toBeNull()
  })

  it('points aria-activedescendant at the highlighted row', async () => {
    const { menu } = await open()
    menu.highlight('downloads')
    await tick()
    const active = surface()?.getAttribute('aria-activedescendant')
    expect(active).toBe(row('downloads')?.id)
  })

  it('shows an empty section as a real, unfocusable state', async () => {
    await open()
    expect(document.body.textContent).toContain('(This section is empty)')
    const empty = document.querySelector('.menu-empty')
    expect(empty?.getAttribute('aria-disabled')).toBe('true')
  })

  // A submenu can hold a toggle, not just actions: a checked child shows the same checkmark a
  // top-level row does, and an unchecked one reserves the column so the labels line up.
  it('shows a checkmark on a checked submenu row, and reserves its column on the others', async () => {
    const { menu } = await open()
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    const submenu = document.querySelector('[data-menu-submenu]')
    const checked = submenu?.querySelector('[data-menu-row="fast"]')
    const unchecked = submenu?.querySelector('[data-menu-row="connect"]')
    expect(checked?.querySelector('.menu-check svg')).not.toBeNull()
    expect(checked?.querySelector('.menu-check-placeholder')).toBeNull()
    expect(unchecked?.querySelector('.menu-check')).toBeNull()
    expect(unchecked?.firstElementChild?.classList.contains('menu-check-placeholder')).toBe(true)
    // Same leading column on every row, check or not, so each label starts at the same x.
    expect(submenu?.querySelectorAll('.menu-check, .menu-check-placeholder')).toHaveLength(3)
  })

  it('marks a submenu parent for assistive tech', async () => {
    const { menu } = await open()
    expect(row('share')?.getAttribute('aria-haspopup')).toBe('menu')
    expect(row('share')?.getAttribute('aria-expanded')).toBe('false')
    menu.surface.openSubmenu('share', true)
    await tick()
    expect(row('share')?.getAttribute('aria-expanded')).toBe('true')
  })
})

/**
 * What counts as "outside". A pointer-down there closes the menu and still reaches what it
 * landed on, which is a deliberate break from the macOS menu (that one swallows the click).
 */
describe('closing on an outside pointer-down', () => {
  /** A chip: the anchor plus a control beside it, the shape the volume switcher's header has. */
  function chip(): { cluster: HTMLElement; anchor: HTMLElement; control: HTMLButtonElement } {
    const cluster = document.createElement('div')
    const anchor = document.createElement('span')
    const control = document.createElement('button')
    cluster.append(anchor, control)
    document.body.appendChild(cluster)
    return { cluster, anchor, control }
  }

  it('closes on a pointer-down somewhere else entirely', async () => {
    const { menu } = await open()
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(menu.isOpen).toBe(false)
  })

  // ❗ The controls BESIDE the anchor belong to the menu too. Without this, pressing the
  // switcher chip's eject button closed the list before the button could act — and ejecting
  // deliberately leaves it open so several drives can go in a row.
  it('stays open for a pointer-down anywhere in the anchor’s control cluster', async () => {
    const { cluster, anchor, control } = chip()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const menu = createMenu({ getSections: sections, onSelect: () => {}, keepOpenWithin: () => cluster })
    menu.openUnder(anchor)
    const component = mount(Menu, { target, props: { menu, ariaLabel: 'Volumes' } })
    await tick()
    mounted.push(() => {
      menu.destroy()
      void unmount(component)
    })

    control.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(menu.isOpen).toBe(true)
    anchor.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(menu.isOpen).toBe(true)
  })

  // ❗ A submenu is a SIBLING of the surface in the portal, not a child, so it isn't caught by
  // the `surfaceEl.contains` test. Without its own exemption, pressing a submenu row closed
  // the menu on pointer-down and the click that followed activated nothing at all.
  it('stays open for a pointer-down in its own submenu', async () => {
    const { menu } = await open()
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    const submenu = document.querySelector('[data-menu-submenu]')
    expect(submenu).not.toBeNull()
    submenu?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(menu.isOpen).toBe(true)
  })
})

/**
 * A menu opened from INSIDE another one: the drive-index badge sits in a volume-switcher row
 * and opens its own menu. The inner menu portals to the body, so a pointer-down in it is not
 * inside the outer surface — and yet it never left the outer menu. Without this rule, clicking
 * the badge's menu closed the switcher out from under it.
 */
describe('a menu opened from inside another', () => {
  /** Mounts a second menu anchored to an element, and returns its controller. */
  async function openNested(anchor: HTMLElement, ariaLabel: string): Promise<MenuController> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const menu = createMenu({ getSections: sections, onSelect: () => {} })
    menu.openUnder(anchor)
    const component = mount(Menu, { target, props: { menu, ariaLabel } })
    await tick()
    await tick()
    mounted.push(() => {
      menu.destroy()
      void unmount(component)
    })
    return menu
  }

  /** A trailing control in every row, the shape a switcher row's drive-index badge has. */
  const badge = createRawSnippet<[MenuRowContext]>(() => ({
    render: () => '<button type="button" class="badge">•</button>',
  }))

  it('leaves the outer menu open when its own surface is used', async () => {
    const { menu: outer } = await open({ trailing: badge })
    const anchor = document.querySelector<HTMLElement>('.badge')
    if (!anchor) throw new Error('expected a trailing badge to anchor from')
    const inner = await openNested(anchor, 'Drive index')

    const innerSurface = document.querySelector('[data-menu][aria-label="Drive index"]')
    expect(innerSurface).not.toBeNull()
    innerSurface?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(inner.isOpen).toBe(true)
    expect(outer.isOpen).toBe(true)
  })

  it('still closes the outer menu for a pointer-down that really left it', async () => {
    const { menu: outer } = await open({ trailing: badge })
    const anchor = document.querySelector<HTMLElement>('.badge')
    if (!anchor) throw new Error('expected a trailing badge to anchor from')
    await openNested(anchor, 'Drive index')

    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(outer.isOpen).toBe(false)
  })
})

describe('pointer selection', () => {
  it('activates the row that was clicked', async () => {
    const onSelect = vi.fn()
    await open({}, { onSelect })
    row('hd')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'hd' }), 'pointer')
  })

  // The whole point of the submenu exemption above: a real click is a pointer-down and then a
  // click, and while the pointer-down closed the menu, the click that followed reached an
  // already-closed controller and picked nothing.
  it('activates a submenu row that was clicked', async () => {
    const onSelect = vi.fn()
    const { menu } = await open({}, { onSelect })
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    const child = document.querySelector('[data-menu-submenu] [data-menu-row="connect"]')
    child?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    child?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'connect' }), 'pointer')
  })

  it('never activates a row from a click on a control inside it', async () => {
    const onSelect = vi.fn()
    const onEject = vi.fn()
    // A trailing button, the shape the switcher's eject control takes.
    const trailing = createRawSnippet<[MenuRowContext]>(() => ({
      render: () => '<button type="button" class="eject">Eject</button>',
    }))
    await open({ trailing }, { onSelect })
    const button = document.querySelector('.eject') as HTMLElement
    button.addEventListener('click', onEject)
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onEject).toHaveBeenCalled()
    // The row must stay put: no call site should need `stopPropagation`.
    expect(onSelect).not.toHaveBeenCalled()
  })
})

/**
 * A submenu row is a full row: the same leading columns, glyph, greyed state, and tooltip the
 * top-level row has, because it's the same markup. The volume switcher's row actions need all
 * of it (a busy drive's Eject is greyed, and every action wears a glyph).
 */
describe('submenu rows', () => {
  function actionSections(): MenuSection[] {
    return [
      {
        id: 'volumes',
        items: [
          {
            value: 'share',
            label: 'Team share',
            submenu: [
              { value: 'eject', label: 'Eject (busy)', icon: { lucide: 'eject' }, disabled: true },
              { value: 'forget', label: 'Forget server', icon: { lucide: 'trash-2' } },
              {
                value: 'fast',
                label: 'Use the fast connection',
                check: { kind: 'toggle', checked: true },
                separatorBefore: true,
              },
            ],
          },
        ],
      },
    ]
  }

  async function openActions(deps: Record<string, unknown> = {}): Promise<Element | null> {
    const { menu } = await open({}, { getSections: actionSections, ...deps })
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    return document.querySelector('[data-menu-submenu]')
  }

  it('greys a disabled submenu row for sight and for assistive tech, and a click picks nothing', async () => {
    const onSelect = vi.fn()
    const submenu = await openActions({ onSelect })
    const eject = submenu?.querySelector<HTMLElement>('[data-menu-row="eject"]')
    expect(eject?.getAttribute('aria-disabled')).toBe('true')
    expect(eject?.hasAttribute('data-disabled')).toBe(true)
    expect(eject?.classList.contains('is-disabled')).toBe(true)
    eject?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onSelect).not.toHaveBeenCalled()
  })

  it('draws a submenu row’s glyph, and holds the glyph column on a row without one', async () => {
    const submenu = await openActions()
    expect(submenu?.querySelector('[data-menu-row="forget"] .menu-icon svg')).not.toBeNull()
    // The toggle has no glyph, so it gets a blank one: every label starts at the same x.
    expect(submenu?.querySelector('[data-menu-row="fast"] .menu-icon-placeholder')).not.toBeNull()
  })

  it('draws a rule above a row that asks for one, and only there', async () => {
    const submenu = await openActions()
    const separators = submenu?.querySelectorAll('[role="separator"]')
    expect(separators).toHaveLength(1)
    expect(separators?.[0].nextElementSibling?.getAttribute('data-menu-row')).toBe('fast')
  })

  // A click on a submenu row must not pull focus off the surface: the submenu unmounts after a
  // pick, and focus on a node that just left the document drops to <body>, where the menu that
  // stayed open (a `keepsMenuOpen` pick) no longer reads as focused.
  it('keeps focus on the menu while a submenu row is pressed', async () => {
    const submenu = await openActions()
    const forget = submenu?.querySelector('[data-menu-row="forget"]')
    const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true })
    forget?.dispatchEvent(press)
    expect(press.defaultPrevented).toBe(true)
  })

  it('opens a row’s submenu on right-click', async () => {
    await open({}, { getSections: actionSections })
    row('share')?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    await tick()
    await tick()
    expect(document.querySelector('[data-menu-submenu] [data-menu-row="forget"]')).not.toBeNull()
  })
})

/**
 * Characterization pins for the screen-reader focus work (#267, #437): the keyboard walk
 * through a rendered menu and its submenu, driven by real keydowns dispatched on whatever
 * holds focus (the menu catches them on its document capture listener), so a focus that
 * fell to `<body>` would still get here and show up in the focus assertions.
 *
 * Focus is pinned as "on a live element inside the menu", not as the parent surface: when
 * submenus learn to speak, focus moves INTO the submenu surface while it has a cursor.
 */
describe('the keyboard walk through a rendered submenu', () => {
  function keepOpenSections(): MenuSection[] {
    return [
      {
        id: 'volumes',
        items: [
          { value: 'hd', label: 'Macintosh HD' },
          {
            value: 'share',
            label: 'Team share',
            submenu: [
              { value: 'eject', label: 'Eject', keepsMenuOpen: true },
              { value: 'forget', label: 'Forget server' },
            ],
          },
        ],
      },
    ]
  }

  /** Dispatches a keydown where a real one lands (on the focused element), then renders. */
  async function press(key: string): Promise<void> {
    const target = document.activeElement ?? document.body
    target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
    await tick()
    await tick()
  }

  function submenu(): HTMLElement | null {
    return document.querySelector('[data-menu-submenu]')
  }

  function highlightedIn(el: Element | null): string | null {
    return el?.querySelector('[data-menu-row][data-highlighted]')?.getAttribute('data-menu-row') ?? null
  }

  /** Focus sits on a connected element inside the menu, never on `<body>`. */
  function expectFocusInMenu(): void {
    const focused = document.activeElement
    expect(focused).not.toBe(document.body)
    expect(focused?.isConnected).toBe(true)
    expect(focused?.closest('[data-menu], [data-menu-submenu]')).not.toBeNull()
  }

  /** Opens the menu, waits for the surface to take focus, and puts the cursor on `share`. */
  async function openOnShare(deps: Record<string, unknown> = {}): Promise<{ menu: MenuController }> {
    const opened = await open({}, deps)
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(surface())
    })
    opened.menu.highlight('share')
    await tick()
    return opened
  }

  it('takes focus on open, on a surface whose active descendant is its highlighted row', async () => {
    await openOnShare()
    assertFocusModel(surface())
    expect(surface()?.getAttribute('aria-activedescendant')).toBe(row('share')?.id)
  })

  it('→ opens and enters the submenu, ↓ and ↑ walk it, ← leaves it on the parent row', async () => {
    await openOnShare()

    await press('ArrowRight')
    expect(submenu()).not.toBeNull()
    expect(highlightedIn(submenu())).toBe('connect')
    expectFocusInMenu()

    await press('ArrowDown')
    expect(highlightedIn(submenu())).toBe('forget')
    await press('ArrowUp')
    expect(highlightedIn(submenu())).toBe('connect')
    expectFocusInMenu()

    await press('ArrowLeft')
    expect(submenu()).toBeNull()
    expect(highlightedIn(surface())).toBe('share')
    expectFocusInMenu()
  })

  it('Enter picks the submenu row the cursor is on, closes, and restores focus once', async () => {
    const onSelect = vi.fn()
    const restoreFocus = vi.fn()
    await openOnShare({ onSelect, restoreFocus })
    await press('ArrowRight')
    await press('ArrowDown')
    await press('Enter')
    expect(onSelect).toHaveBeenCalledTimes(1)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'forget' }), 'keyboard')
    expect(surface()).toBeNull()
    expect(restoreFocus).toHaveBeenCalledTimes(1)
  })

  it('a keepsMenuOpen pick closes only the submenu, and focus stays on a live element in the menu', async () => {
    const onSelect = vi.fn()
    const restoreFocus = vi.fn()
    await openOnShare({ getSections: keepOpenSections, onSelect, restoreFocus })
    await press('ArrowRight')
    expect(highlightedIn(submenu())).toBe('eject')
    await press('Enter')
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'eject' }), 'keyboard')
    expect(surface()).not.toBeNull()
    expect(submenu()).toBeNull()
    expect(highlightedIn(surface())).toBe('share')
    expectFocusInMenu()
    expect(restoreFocus).not.toHaveBeenCalled()
  })

  it('Escape closes the submenu first, then the menu, restoring focus once', async () => {
    const restoreFocus = vi.fn()
    await openOnShare({ restoreFocus })
    await press('ArrowRight')
    expect(submenu()).not.toBeNull()

    await press('Escape')
    expect(submenu()).toBeNull()
    expect(surface()).not.toBeNull()
    expect(highlightedIn(surface())).toBe('share')
    expectFocusInMenu()
    expect(restoreFocus).not.toHaveBeenCalled()

    await press('Escape')
    expect(surface()).toBeNull()
    expect(restoreFocus).toHaveBeenCalledTimes(1)
  })
})

/**
 * What VoiceOver is told. Focus sits on the surface that owns the cursor and that surface's
 * `aria-activedescendant` names the cursor row: the top-level surface while the parent list
 * has the cursor, the submenu surface once the cursor is inside it. Every id is unique in the
 * document, and the attribute lands a frame after focus, because VoiceOver reads only the
 * container when the two arrive together.
 */
describe('what a screen reader is told', () => {
  /** A submenu child that shares its value with a top-level row, and one that's all disabled. */
  function srSections(): MenuSection[] {
    return [
      {
        id: 'volumes',
        items: [
          { value: 'hd', label: 'Macintosh HD' },
          {
            value: 'share',
            label: 'Team share',
            submenu: [
              { value: 'hd', label: 'Open on Macintosh HD' },
              { value: 'forget', label: 'Forget server' },
              { value: 'stay', label: 'Eject', keepsMenuOpen: true },
            ],
          },
          {
            value: 'busy',
            label: 'Busy drive',
            submenu: [{ value: 'eject', label: 'Eject (busy)', disabled: true }],
          },
        ],
      },
    ]
  }

  async function press(key: string): Promise<void> {
    const target = document.activeElement ?? document.body
    target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
    await tick()
    await tick()
  }

  function submenu(): HTMLElement | null {
    return document.querySelector('[data-menu-submenu]')
  }

  function subrow(value: string): HTMLElement | null {
    return document.querySelector(`[data-menu-submenu] [data-menu-row="${value}"]`)
  }

  /** Opens on `row`, waits for the surface's focus and its active descendant. */
  async function openOn(value: string, deps: Record<string, unknown> = {}): Promise<{ menu: MenuController }> {
    const opened = await open({}, { getSections: srSections, ...deps })
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(surface())
    })
    opened.menu.highlight(value)
    await tick()
    return opened
  }

  /** Walks into `share`'s submenu with →, and waits for its surface to take focus. */
  async function enterShare(deps: Record<string, unknown> = {}): Promise<{ menu: MenuController }> {
    const opened = await openOn('share', deps)
    await press('ArrowRight')
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(submenu())
    })
    return opened
  }

  it('gives submenu rows document-unique ids, even where a value repeats a top-level one', async () => {
    const { menu } = await openOn('share')
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    const ids = [...document.querySelectorAll('[data-menu-row]')].map((el) => el.id)
    expect(ids.every((id) => id !== '')).toBe(true)
    expect(new Set(ids).size).toBe(ids.length)
    expect(subrow('hd')?.id).not.toBe(row('hd')?.id)
  })

  it('names the submenu after its row, and the row says which surface it opened', async () => {
    const { menu } = await openOn('share')
    expect(row('share')?.hasAttribute('aria-controls')).toBe(false)
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    expect(submenu()?.getAttribute('aria-label')).toBe('Team share')
    expect(submenu()?.getAttribute('tabindex')).toBe('-1')
    expect(submenu()?.id).toBeTruthy()
    expect(row('share')?.getAttribute('aria-controls')).toBe(submenu()?.id)
  })

  it('lands the surface’s active descendant a frame after it takes focus on open', async () => {
    const atFocus: (string | null)[] = []
    document.addEventListener('focusin', (event) => {
      const el = event.target as HTMLElement
      if (el.matches('[data-menu]')) atFocus.push(el.getAttribute('aria-activedescendant'))
    })
    await open({}, { getSections: srSections })
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(surface())
    })
    expect(atFocus).toEqual([null])
    await vi.waitFor(() => {
      expect(surface()?.getAttribute('aria-activedescendant')).toBe(row('hd')?.id)
    })
    assertFocusModel(surface())
  })

  it('→ moves focus into the submenu once it’s placed, and its active descendant follows ↓', async () => {
    const visibilityAtFocus: string[] = []
    document.addEventListener('focusin', (event) => {
      const el = event.target as HTMLElement
      if (el.matches('[data-menu-submenu]')) visibilityAtFocus.push(el.style.visibility)
    })
    await enterShare()
    // `focus()` on a `visibility: hidden` node does nothing in WebKit, so it waits for placement.
    expect(visibilityAtFocus).toEqual(['visible'])
    await vi.waitFor(() => {
      expect(submenu()?.getAttribute('aria-activedescendant')).toBe(subrow('hd')?.id)
    })
    assertFocusModel(submenu())

    await press('ArrowDown')
    expect(submenu()?.getAttribute('aria-activedescendant')).toBe(subrow('forget')?.id)
    assertFocusModel(submenu())
  })

  it('leaves focus in the submenu: the parent surface never takes it back', async () => {
    await enterShare()
    await new Promise((resolve) => setTimeout(resolve, 50))
    await tick()
    expect(document.activeElement).toBe(submenu())
  })

  /** Focus on a node that leaves the document drops to `<body>`, and the next key goes nowhere useful. */
  describe('focus returns to the parent surface before the submenu goes away', () => {
    /** Records where focus was at the moment the submenu node left the document. */
    function watchSubmenuRemoval(): { focusAtRemoval: () => Element | null | undefined } {
      let seen: Element | null | undefined
      const node = submenu()
      const observer = new MutationObserver(() => {
        if (node && !node.isConnected && seen === undefined) {
          seen = document.activeElement
          observer.disconnect()
        }
      })
      observer.observe(document.body, { childList: true, subtree: true })
      // The blur fires synchronously on removal of a focused node, before any observer.
      node?.addEventListener('blur', () => {
        if (seen === undefined && !node.isConnected) seen = document.body
      })
      return { focusAtRemoval: () => seen }
    }

    it('on ←', async () => {
      await enterShare()
      const watch = watchSubmenuRemoval()
      await press('ArrowLeft')
      expect(submenu()).toBeNull()
      expect(document.activeElement).toBe(surface())
      expect(watch.focusAtRemoval()).toBe(surface())
      assertFocusModel(surface())
      expect(surface()?.getAttribute('aria-activedescendant')).toBe(row('share')?.id)
    })

    it('on Escape', async () => {
      await enterShare()
      await press('Escape')
      expect(submenu()).toBeNull()
      expect(document.activeElement).toBe(surface())
    })

    it('on a keepsMenuOpen pick', async () => {
      await enterShare()
      await press('ArrowDown')
      await press('ArrowDown')
      await press('Enter')
      expect(submenu()).toBeNull()
      expect(document.activeElement).toBe(surface())
    })

    it('when the pointer drifts off a keyboard-entered submenu', async () => {
      await enterShare()
      submenu()?.dispatchEvent(new MouseEvent('mouseleave'))
      await tick()
      await tick()
      expect(submenu()).toBeNull()
      expect(document.activeElement).toBe(surface())
    })
  })

  it('a hover-opened submenu shows but doesn’t take focus, until the pointer enters it', async () => {
    await openOn('hd')
    row('share')?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    await tick()
    await tick()
    expect(submenu()).not.toBeNull()
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(document.activeElement).toBe(surface())

    subrow('forget')?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(submenu())
    })
  })

  // "Why can't I eject this?": its greyed rows are the answer, so VoiceOver must reach them.
  it('a keyboard-opened all-disabled submenu takes focus and names its first row, cursor still null', async () => {
    const { menu } = await openOn('busy')
    await press('ArrowRight')
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(submenu())
    })
    expect(menu.submenuHighlightedValue).toBeNull()
    await vi.waitFor(() => {
      expect(submenu()?.getAttribute('aria-activedescendant')).toBe(subrow('eject')?.id)
    })
    expect(subrow('eject')?.getAttribute('aria-disabled')).toBe('true')
  })
})

/**
 * The `data-*` hooks are a contract other suites select on (`lib/ui/DETAILS.md` § Menu),
 * so they're asserted here rather than left to rot as decoration.
 */
describe('test hooks', () => {
  it('names the surface, each row by value, and the checked and disabled states', async () => {
    await open()
    expect(surface()).not.toBeNull()
    expect(row('hd')?.hasAttribute('data-checked')).toBe(true)
    expect(row('backup')?.hasAttribute('data-disabled')).toBe(true)
    expect(row('projects')?.hasAttribute('data-checked')).toBe(false)
    expect(document.querySelector('[data-menu-section="volumes"]')).not.toBeNull()
    expect(document.querySelector('[data-menu-section="volumes"] [data-menu-heading]')?.textContent).toBe('Volumes')
    expect(document.querySelector('[data-menu-empty]')).not.toBeNull()
  })

  it('gives every surface an instance name, and names the host of a nested one', async () => {
    const trailing = createRawSnippet<[MenuRowContext]>(() => ({
      render: () => '<button type="button" class="badge">•</button>',
    }))
    await open({ trailing })
    const outerId = surface()?.getAttribute('data-menu-instance')
    expect(outerId).toBeTruthy()
    // A menu anchored to a row's own control names the surface it came from; a top-level one
    // (the outer menu here, opened at a point) names nothing.
    expect(surface()?.hasAttribute('data-menu-nested-in')).toBe(false)

    const anchor = document.querySelector<HTMLElement>('.badge')
    if (!anchor) throw new Error('expected a trailing badge to anchor from')
    const target = document.createElement('div')
    document.body.appendChild(target)
    const inner = createMenu({ getSections: sections, onSelect: () => {} })
    inner.openUnder(anchor)
    const component = mount(Menu, { target, props: { menu: inner, ariaLabel: 'Drive index' } })
    await tick()
    mounted.push(() => {
      inner.destroy()
      void unmount(component)
    })
    expect(document.querySelector('[aria-label="Drive index"]')?.getAttribute('data-menu-nested-in')).toBe(outerId)
  })

  it('moves data-highlighted with the cursor, and marks keyboard mode', async () => {
    const { menu } = await open()
    menu.highlight('downloads')
    await tick()
    expect(document.querySelector('[data-menu-row][data-highlighted]')?.getAttribute('data-menu-row')).toBe('downloads')
    expect(surface()?.hasAttribute('data-keyboard-mode')).toBe(false)
    menu.handleKey(new KeyboardEvent('keydown', { key: 'ArrowDown', cancelable: true }))
    await tick()
    expect(surface()?.hasAttribute('data-keyboard-mode')).toBe(true)
  })

  it('names the submenu surface and its highlighted row', async () => {
    const { menu } = await open()
    menu.surface.openSubmenu('share', true)
    // Twice: the submenu renders only once its position effect has measured the parent row,
    // and that effect resolves inside a `tick().then(...)` of its own.
    await tick()
    await tick()
    const submenu = document.querySelector('[data-menu-submenu]')
    expect(submenu).not.toBeNull()
    expect(submenu?.querySelector('[data-menu-row="connect"][data-highlighted]')).not.toBeNull()
    // Exactly one row lights up: a boolean here lit every row of a multi-item submenu.
    expect(submenu?.querySelectorAll('[data-highlighted]')).toHaveLength(1)
  })

  it('marks the checked submenu row, and only that one', async () => {
    const { menu } = await open()
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    const submenu = document.querySelector('[data-menu-submenu]')
    expect(submenu?.querySelector('[data-menu-row="fast"]')?.hasAttribute('data-checked')).toBe(true)
    expect(submenu?.querySelector('[data-menu-row="connect"]')?.hasAttribute('data-checked')).toBe(false)
  })

  it('marks the dragged row, and the cue row carries its insertion slot', async () => {
    const { menu } = await open()
    // Two favorite rows 20px tall from y=0: midpoints 10 and 30.
    menu.surface.bindSurface({ getRowMidpoints: () => [10, 30] })
    menu.surface.startDrag('projects', new MouseEvent('mousedown', { clientY: 10, button: 0 }))
    window.dispatchEvent(new MouseEvent('mousemove', { clientY: 45 }))
    await tick()
    expect(row('projects')?.hasAttribute('data-dragging')).toBe(true)
    // Dropping past the last row: the cue sits below it, at slot 2.
    const cueRow = document.querySelector('[data-drop-cue]')
    expect(cueRow?.getAttribute('data-menu-row')).toBe('downloads')
    expect(cueRow?.getAttribute('data-drop-cue')).toBe('below')
    expect(cueRow?.getAttribute('data-drop-slot')).toBe('2')
    window.dispatchEvent(new MouseEvent('mouseup', { clientY: 45 }))
  })

  /**
   * ❗ The regression anchor for the surface REGISTERING its row measurement. Every other
   * drag test here hands `getRowMidpoints` in by calling `bindSurface` itself, which is
   * exactly how the component going without it stayed invisible: with no measurement the
   * controller reads an empty midpoint list, `pointerReorderTarget` answers "no target"
   * for every row, and a drop puts the row back where it started. ❌ Don't add
   * `bindSurface` to this one.
   */
  it('reorders a real drag with NO hand-registered measurement: the surface measures its own rows', async () => {
    const onReorder = vi.fn()
    await open({}, { onReorder })
    // The test DOM has no layout engine, so the rows get 20px rects by hand: midpoints
    // 10 and 30.
    ;['projects', 'downloads'].forEach((value, index) => {
      const top = index * 20
      const el = row(value)
      if (el) {
        el.getBoundingClientRect = () =>
          ({ top, bottom: top + 20, height: 20, left: 0, right: 200, width: 200, x: 0, y: top }) as DOMRect
      }
    })

    row('projects')?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientY: 10 }))
    window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientY: 45 }))
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientY: 45 }))
    await tick()

    expect(onReorder).toHaveBeenCalledWith({
      sectionId: 'favorites',
      orderedValues: ['downloads', 'projects'],
      from: 0,
      to: 1,
    })
  })
})

/**
 * The accelerator column: leftmost, present only where something uses it, and reserved on every
 * row of such a menu so the labels stay in one line.
 */
describe('the accelerator column', () => {
  /** Mixed on purpose: two rows carry a digit and one doesn't, which is what alignment is about. */
  function accelerated(): MenuSection[] {
    return [
      {
        id: 'favorites',
        heading: 'Favorites',
        items: [
          { value: 'projects', label: 'Projects', accelerator: '1' },
          { value: 'downloads', label: 'Downloads', accelerator: '2' },
          { value: 'elsewhere', label: 'Somewhere else' },
        ],
      },
    ]
  }

  it('renders no column at all in a menu where nothing declares an accelerator', async () => {
    await open()
    expect(document.querySelector('.menu-accelerator')).toBeNull()
    expect(document.querySelector('.menu-accelerator-placeholder')).toBeNull()
  })

  it('shows the digit first, before the checkmark column', async () => {
    await open({}, { getSections: accelerated })
    const first = row('projects')?.firstElementChild
    expect(first?.classList.contains('menu-accelerator')).toBe(true)
    expect(first?.textContent).toBe('1')
  })

  it('reserves a blank column on the rows without one, so every label lines up', async () => {
    await open({}, { getSections: accelerated })
    // Every row leads with a 14px column, digit or not; without the placeholder this row's
    // label would sit one column left of the other two.
    expect(row('elsewhere')?.firstElementChild?.classList.contains('menu-accelerator-placeholder')).toBe(true)
    expect(document.querySelectorAll('.menu-accelerator, .menu-accelerator-placeholder')).toHaveLength(3)
  })

  it('carries data-accelerator on the row, and says the shortcut to assistive tech', async () => {
    await open({}, { getSections: accelerated })
    expect(row('downloads')?.getAttribute('data-accelerator')).toBe('2')
    expect(row('downloads')?.getAttribute('aria-keyshortcuts')).toBe('2')
    expect(row('elsewhere')?.hasAttribute('data-accelerator')).toBe(false)
    // The glyph is decoration: `aria-keyshortcuts` is what a screen reader reads.
    expect(row('downloads')?.querySelector('.menu-accelerator')?.getAttribute('aria-hidden')).toBe('true')
  })
})

describe('snippets', () => {
  it('label replaces the row text', async () => {
    const label = createRawSnippet<[MenuRowContext]>((context) => ({
      render: () => `<span class="custom-label">Renaming ${context().item.label}</span>`,
    }))
    await open({ label })
    expect(document.querySelector('.custom-label')?.textContent).toContain('Renaming Projects')
  })

  it('below adds a sub-line under the row, and footer sits under the last section', async () => {
    const below = createRawSnippet<[MenuRowContext]>((context) => ({
      render: () => (context().item.value === 'hd' ? '<div class="space">312 GB free</div>' : '<div></div>'),
    }))
    const footer = createRawSnippet(() => ({ render: () => '<div class="footer">Still loading</div>' }))
    await open({ below, footer })
    expect(document.querySelector('.space')?.textContent).toBe('312 GB free')
    expect(document.querySelector('.footer')?.textContent).toBe('Still loading')
  })
})

/**
 * A checkmarked row tells assistive tech what it means. A toggle is a `menuitemcheckbox`
 * whether it's on or off (an unchecked toggle is still a toggle, and VoiceOver says
 * "unchecked"); the "you are here" row stays a `menuitem` with `aria-current`, so the
 * controls it hosts stay reachable; a row with no `check` is a plain `menuitem` with neither.
 * Top-level and submenu rows share the markup, so both get pinned.
 */
describe('checkable rows', () => {
  function checkSections(): MenuSection[] {
    return [
      {
        id: 'volumes',
        items: [
          { value: 'hd', label: 'Macintosh HD', check: { kind: 'current' } },
          { value: 'backup', label: 'Backup' },
          { value: 'hidden', label: 'Show hidden files', check: { kind: 'toggle', checked: false } },
          { value: 'plain', label: 'Plain row' },
          {
            value: 'share',
            label: 'Team share',
            submenu: [
              { value: 'open', label: 'Open' },
              { value: 'fast', label: 'Use the fast connection', check: { kind: 'toggle', checked: true } },
              { value: 'reconnect', label: 'Reconnect automatically', check: { kind: 'toggle', checked: false } },
            ],
          },
        ],
      },
    ]
  }

  /** `[role, aria-checked, aria-current]`, absent attributes as `null`. */
  function a11y(el: Element | null | undefined): (string | null)[] {
    return ['role', 'aria-checked', 'aria-current'].map((name) => el?.getAttribute(name) ?? null)
  }

  it('announces top-level toggle, current, and plain rows with the right role and state', async () => {
    await open({}, { getSections: checkSections })
    expect(a11y(row('hidden'))).toEqual(['menuitemcheckbox', 'false', null])
    expect(a11y(row('hd'))).toEqual(['menuitem', null, 'location'])
    expect(a11y(row('backup'))).toEqual(['menuitem', null, null])
    expect(a11y(row('plain'))).toEqual(['menuitem', null, null])
    expect(a11y(row('share'))).toEqual(['menuitem', null, null])
  })

  it('announces submenu toggles as checkboxes, on and off, and leaves actions plain', async () => {
    const { menu } = await open({}, { getSections: checkSections })
    menu.surface.openSubmenu('share', true)
    await tick()
    await tick()
    const submenu = document.querySelector('[data-menu-submenu]')
    expect(a11y(submenu?.querySelector('[data-menu-row="fast"]'))).toEqual(['menuitemcheckbox', 'true', null])
    expect(a11y(submenu?.querySelector('[data-menu-row="reconnect"]'))).toEqual(['menuitemcheckbox', 'false', null])
    expect(a11y(submenu?.querySelector('[data-menu-row="open"]'))).toEqual(['menuitem', null, null])
  })

  it('draws the checkmark and `data-checked` only on an on toggle or the current row', async () => {
    await open({}, { getSections: checkSections })
    expect(row('hd')?.hasAttribute('data-checked')).toBe(true)
    expect(row('hd')?.querySelector('.menu-check svg')).not.toBeNull()
    expect(row('backup')?.hasAttribute('data-checked')).toBe(false)
    expect(row('hidden')?.querySelector('.menu-check')).toBeNull()
    expect(row('hidden')?.querySelector('.menu-check-placeholder')).not.toBeNull()
  })
})

/**
 * A section folded behind a disclosure row. The row says whether it's open (to VoiceOver
 * through `aria-expanded`, to a test through `data-menu-disclosure`), says how many rows it
 * holds, and the rows render only while it's open.
 */
describe('disclosure sections', () => {
  function foldedSections(expanded: boolean): () => MenuSection[] {
    return () => [
      {
        id: 'favorites',
        disclosure: { expanded, label: 'Favorites', icon: { lucide: 'star' } },
        items: [
          { value: 'projects', label: 'Projects' },
          { value: 'downloads', label: 'Downloads' },
        ],
      },
      { id: 'volumes', items: [{ value: 'hd', label: 'Macintosh HD' }] },
    ]
  }

  const HEADER = disclosureRowValue('favorites')

  it('renders a collapsed section as its disclosure row alone, with a count', async () => {
    await open({}, { getSections: foldedSections(false) })
    const header = row(HEADER)
    expect(header?.getAttribute('role')).toBe('menuitem')
    expect(header?.getAttribute('aria-expanded')).toBe('false')
    expect(header?.getAttribute('data-menu-disclosure')).toBe('collapsed')
    expect(header?.textContent).toContain('Favorites')
    expect(header?.querySelector('[data-menu-disclosure-count]')?.textContent).toBe('2')
    expect(row('projects')).toBeNull()
    expect(row('hd')).not.toBeNull()
  })

  it('renders the rows under an expanded disclosure row', async () => {
    await open({}, { getSections: foldedSections(true) })
    expect(row(HEADER)?.getAttribute('aria-expanded')).toBe('true')
    expect(row(HEADER)?.getAttribute('data-menu-disclosure')).toBe('expanded')
    expect(row('projects')).not.toBeNull()
    expect(row('downloads')).not.toBeNull()
  })

  it('hands a click on the disclosure row to the caller, and stays open', async () => {
    const onDisclosureChange = vi.fn()
    const onSelect = vi.fn()
    const { menu } = await open({}, { getSections: foldedSections(false), onDisclosureChange, onSelect })
    row(HEADER)?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onDisclosureChange).toHaveBeenCalledWith({ sectionId: 'favorites', expanded: true })
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.isOpen).toBe(true)
  })

  it('lets the caller decorate the disclosure row’s right end', async () => {
    const disclosureTrailing = createRawSnippet<[MenuSection]>(() => ({
      render: () => '<span class="chip">⌃D</span>',
    }))
    await open({ disclosureTrailing }, { getSections: foldedSections(false) })
    expect(row(HEADER)?.querySelector('.chip')).not.toBeNull()
  })
})
