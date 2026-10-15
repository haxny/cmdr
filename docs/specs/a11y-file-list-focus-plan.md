# File list and menu focus model for screen readers (#437, #267)

Make the file list and the house menus speak to VoiceOver without changing a single keystroke's behavior. Issues: #437
(file list), #267 (menus; David's VoiceOver run: top-level volume switcher and Favorites read decently, submenus are
silent). Background: `docs/notes/accessibility-audit-2026-10.md`, David's guide (vault,
`projects/Cmdr/2026-10-14 Cmdr VoiceOver test guide.md`, checks V1–V12).

## Loud rules

1. ❗ **Keyboard behavior is frozen.** `Tab` switches panes, every shortcut does what it does today, in the same pane,
   with the same cursor. Characterization tests land FIRST (M1) and must stay green untouched through every later
   milestone, except the assertions this plan names as deliberate changes (each listed in its milestone).
2. ❗ **One focus model, everywhere:** DOM focus sits on the composite widget that owns the cursor, and that element's
   `aria-activedescendant` names the cursor row by a document-unique id. Keyboard ROUTING never depends on which of
   these elements holds focus (it's already container- and document-level; keep it so).
3. ❌ **Never move DOM focus into a file row.** Rows unmount under virtualization; focus on a removed node drops to
   `<body>` and every pane key dies.
4. ❌ **Never call `.focus()` on a list without `{ preventScroll: true }`.** The listbox is taller than its scroller;
   WebKit would scroll it to its top and yank the view off the cursor.
5. ❌ **Never translate strings you add.** English plus a good `@key` description (`AGENTS.md` § Hard rules).
6. **Coordinate with #442** (in flight in this same worktree): it folds sync/symlink badge meaning into the row's
   accessible name and moves the rename field's hard-coded `Rename ${name}` label into the catalog. M4 builds ON its row
   name helper; rebase onto it first, don't fork a second one.
7. Strip milestone tags from code, comments, test names, and docs before merging
   (`docs/guides/multi-agent-refactors.md`).

## Current-code facts

Paths are under `apps/desktop/src/` unless noted. Line numbers as of `008c53c6b`.

**Focus today**

- The explorer container holds focus: `lib/file-explorer/pane/DualPaneExplorer.svelte:1474-1484` (`tabindex="0"`,
  `role="application"`, `aria-label="File explorer"`, `onfocusin={keyDispatch.handleFocusGuard}`, `onkeydown`,
  `onkeyup`).
- The guard: `lib/file-explorer/pane/key-dispatch.ts:77-90`. Anything focused inside the container except the container
  itself, inputs/textareas/selects/contenteditable, and dialog content gets bounced back to the container. Why the
  dialog exemption is load-bearing: same file, 67-76, and `pane/DETAILS.md` ~1594.
- `containerElement?.focus()` is THE refocus primitive, called at `DualPaneExplorer.svelte` 202 (`onRefocus`, fanned out
  to `dialog-state`, `adopted-operation`, `archive-password-flow`), 253, 271, 285 (`focusContainer` in three
  `PaneAccess`-shaped deps), 542 (`handleFocus`, the tail of every `onRequestFocus`/`onPaneFocus`), 838 (post-init
  `$effect`), 852 (`refocus()`, the public API `+page.svelte` calls after modals), 865 (`switchPane`), 875
  (`switchVolumeChooserPane`), 1106 (`moveCursor`). `PaneAccess.focusContainer` (`pane/pane-access.ts:27`) carries it
  into command factories (`swap-panes.ts`, `move-cursor.ts`, `edge-flow-handlers.ts`). The exported `setFocusedPane`
  (1028) is store-only and touches no DOM.
- Two callers bypass the primitive and focus the container by selector: `lib/ask-cmdr/ask-cmdr-trigger.svelte.ts:117`
  (`returnFocusToPane`) and `pane/NetworkMountView.svelte:262`. E2E helpers do the same (`helpers/cursor.ts`,
  `helpers/app-lifecycle.ts:299-314`).
- Generic focus RESTORERS remember `document.activeElement` at open and call a bare `.focus()` on it at close:
  `lib/command-palette/CommandPalette.svelte:99, 113-114`, `lib/ui/ModalDialog.svelte:478, 517-518`,
  `lib/onboarding/OnboardingWizard.svelte:75, 108-109`, `lib/ui/toast/ToastItem.svelte:84-115`, plus the menu consumers
  below. Today the remembered element is always the pane-neutral container, so this is harmless.
- A mouse pane switch happens on CLICK: `FilePane`'s `onclick` → `pane-pointer.ts:174-181` `handlePaneClick` →
  `onRequestFocus` → `handleFocus`. The browser's mousedown default (focus the nearest focusable ancestor of the row,
  i.e. the clicked pane's listbox or scroller) runs BEFORE that, while the store still names the old pane; the guard
  bounces it today.
- Nothing suppresses native keyboard scrolling (`global-keydown.ts:70` `shouldSuppressKey` only covers ⌘A and ⌥⌘I).
  Today focus sits on the container, which has no scrollable ancestor, so an unclaimed arrow, Page, Home/End, or Space
  scrolls nothing.
- Pane keys: `key-dispatch.ts:102-134` forwards to the FOCUSED pane (explorer store), regardless of the event target,
  except text inputs (`isTypingInInput`). Global commands: `routes/(main)/+page.svelte:416` document bubble listener →
  `routes/(main)/global-keydown.ts:119` `resolveGlobalKeyAction`, which only looks at `isTextInputFocused()`. `Tab` is
  `pane.switch` (`lib/commands/sources/main-window.ts:283`), dispatched with `preventDefault`.
- The focus watchdog counts focus anywhere inside `.dual-pane-explorer, .ask-cmdr-rail, [data-menu]` as home
  (`lib/focus-watchdog.ts:37`, checked at 47). The submenu surface carries `data-menu-submenu`, not `data-menu`, and is
  a sibling of the parent surface, so focus on it would read as "left home".
- E2E depends on the container being `activeElement`: `test/e2e-playwright/app.spec.ts:146` asserts it exactly;
  `test/e2e-playwright/helpers/cursor.ts:60-75` (`ensureExplorerFocused`) accepts anything INSIDE the container.

**The file list**

- `lib/file-explorer/views/BriefList.svelte:940-953`: `.brief-list` is scroller AND `role="listbox"`, `tabindex="-1"`,
  `aria-label="File list"`, `aria-activedescendant`. Rows 976-1002: `id={`file-${globalIndex}`}`, `role="option"`,
  `aria-selected={globalIndex === cursorIndex}`. Rendered-only activedescendant: 470-480.
- `lib/file-explorer/views/FullList.svelte:701-721`: scroller `.full-list` (`tabindex="-1"`) wraps `.listbox-region`
  (`role="listbox"`, `tabindex="-1"`, same label and activedescendant). Rows 746-769 identical id/role/aria-selected.
  Rendered-only activedescendant: 514-523. Why the listbox sits on the inner wrapper: `views/DETAILS.md` ~316-327.
- ❗ **Row ids collide across panes.** Both panes (and `SearchResultsView`, which reuses `FullList`) render `file-0`,
  `file-1`, … `getElementById` returns the first in DOM order, so even a correctly focused RIGHT listbox would announce
  the LEFT pane's row. Fix this before anything else in the list.
- Marked state lives only in a CSS class (`selectedIndices.has(globalIndex)` → `is-selected`); `aria-selected` is the
  cursor. No `aria-multiselectable`, no `aria-setsize`/`aria-posinset`.
- Row content: `FileIcon` (`alt=""`), name text, `StatusGlyph`s with labels, `TagDots`; in Full view also git, ext,
  size, date cells concatenated with no column names. `FileEntry` has `isDirectory`, `isSymlink`, `isArchive?`
  (`lib/file-explorer/types.ts:17+`); the `..` row comes from `createParentEntry` (`views/file-list-utils.ts:124`,
  `name: '..'`). `totalCount` includes the `..` row. There's no typed package flag:
  `cmdr_fs::icons::packages:: is_package_dir` decides it in Rust and reaches the frontend only as the `pkg:` prefix of
  `iconId`.
- One list per pane at a time (`FilePane` is `{#key}`ed on the active tab, `DualPaneExplorer.svelte` ~1395), plus
  `SearchResultsView`'s `FullList` when results replace a pane's list.
- Strings: `lib/intl/messages/en/fileExplorer.json` 46 (`filePaneAriaLabel`, "Left/Right file pane"), 136
  (`fileExplorerAriaLabel`), 1358 (`fileListAriaLabel`, "File list").
- Pane region: `lib/file-explorer/pane/FilePane.svelte:1995-2003` (`role="region"`, label "Left file pane"). Header
  (volume chip, path segment `<button>`s, repo chip) 2009-2026. Tab bar is a real `role="tablist"`
  (`lib/file-explorer/tabs/TabBar.svelte:125`). `FullListHeader.svelte:50` omits roles on purpose.
- Volume chooser trigger: `lib/file-explorer/navigation/VolumeBreadcrumb.svelte:201-227`, a `<span onclick>`, not
  focusable, no `aria-haspopup`/`aria-expanded`. Its sibling `.breadcrumb-options-trigger` (238-255) and the
  `.breadcrumb-popup-item` (259-268) are the same shape.

**The house menu (`lib/ui/Menu.svelte`, `lib/ui/menu-controller.svelte.ts`)**

- Instance-unique row ids (`Menu.svelte:6-7, 75-79`), surface `role="menu"` with `tabindex="-1"` and
  `aria-activedescendant={rowId(menu.highlightedValue)}` (364-377), focused on open by the `surfaceEl`-keyed effect
  (151-170). Keys come through a document CAPTURE listener (`menu-controller.svelte.ts:198`) and a stack of open menus,
  so focus location never routes keys. This is the pattern David heard working.
- **Why submenus are silent** (all four hold; any one would do it):
  1. Submenu rows render NO `id` (`Menu.svelte:548-575`), so nothing can name them.
  2. The surface's `aria-activedescendant` is bound to `menu.highlightedValue` (376), which stays on the PARENT row
     while the submenu cursor moves in a separate `submenuHighlightedValue`
     (`menu-controller.svelte.ts:166-169, 261-274, 591-598`). The active descendant never changes as you arrow, so
     WebKit posts nothing and VoiceOver says nothing.
  3. Even with an id, the submenu is a SIBLING of the focused surface in the portal (`Menu.svelte:526-533`), not a
     descendant, and nothing `aria-owns` it; activedescendant must name a descendant or owned element.
  4. The submenu reuses the parent's `aria-label` (534) and the parent row has no `aria-controls`, so even an announced
     submenu wouldn't say which row it belongs to.
- Submenu lifecycle: `onmouseleave` on the submenu surface closes it (`Menu.svelte` ~538); a submenu whose every row is
  disabled opens CURSORLESS even from the keyboard (`menu-controller.svelte.ts:261-268`, the "why can't I eject this?"
  case).
- Consumers restore focus by remembering `document.activeElement` at open: `VolumeChooserMenu.svelte:285, 340-355`,
  `FavoritesMenu.svelte:85, 123-139`, `DriveIndexBadge.svelte:220-238`, `network/ServersHubRowMenu.svelte:59`;
  `pane/enter-menu.svelte.ts:26, 56` takes a `restoreFocus` dep.

## The decision

**Active descendant on the element that really has focus, and that element is the composite itself:**

- **File list:** DOM focus moves from the container to the FOCUSED pane's listbox (the element that already carries
  `role="listbox"` and `tabindex="-1"`). `Tab` (`switchPane`) moves it to the other pane's listbox. The container stays
  as the fallback home when the focused pane shows no list (loading, error, network, Servers hub views) and keeps every
  key handler, since keydown/keyup/focusin bubble up to it from the listbox.
- **Menus:** the top-level surface keeps focus as today. When the submenu has a cursor (`→`, or the pointer entered it),
  DOM focus moves to the submenu surface, which carries its own `aria-activedescendant`; when the cursor leaves it (`←`,
  Escape, a pick, hover back to the parent), focus returns to the parent surface BEFORE the submenu unmounts.

Why this one:

- **It's the shape already proven in this exact WKWebView.** The top-level menu (focused `role="menu"` +
  `aria-activedescendant`) reads under VoiceOver; the command palette and onboarding pickers use the same pattern. The
  file list is broken only because focus and the attribute sit on different elements, plus the id collision.
- **Virtualization is a non-issue.** Rows can mount and unmount freely; focus never sits on one. The rendered-only
  activedescendant rule (`views/DETAILS.md`) keeps working unchanged.
- **Keyboard behavior can't move.** The listbox is inside the container, so `onkeydown`/`onkeyup` still fire there, the
  document-level dispatcher still sees every key, and the routing target is still `explorerState.getFocusedPane()`, not
  the event target. Pane switching is focus moving between two descendants, so VoiceOver reads the new pane's cursor row
  for free. It never reads the LIST's name (M0), so which pane and folder you're in comes from a per-pane polite live
  region (M4 item 4).

Rejected:

- **Activedescendant on the container** (keep focus where it is, point the `application` container at the row): the
  smallest diff, but whether WebKit honors `aria-activedescendant` on `role="application"` and announces an option
  inside an unfocused listbox is unverified, and the referenced option would be announced without its list's context.
  Kept as the FALLBACK if the M0 spike shows WebKit mishandling the chosen model.
- **Roving tabindex / real focus on rows** (#267's suggested fix): rows unmount under virtualization (scroll away with
  the wheel and focus falls to `<body>`), every cursor step becomes a focus/blur storm through the guard, the watchdog,
  and the rename editor's blur logic, and `focus()` scroll-into-view fights the virtual scroller. For menus it would
  work, but one model for both is the point, and the menu bug is a missing id, not the model.
- **`aria-owns` from the parent menu to the submenu** instead of moving focus: WebKit's reparenting for `aria-owns`
  under VoiceOver is the less-trodden path; moving focus between two `role="menu"` surfaces is what native submenus do.
- **Full view as `role="grid"`**: Cmdr's cursor is a row, never a cell. A grid with activedescendant needs a focused
  cell (or a treegrid row model), and VoiceOver table navigation over a virtualized grid (`aria-rowcount` /
  `aria-rowindex`) is a second untested surface. Both views stay listboxes; Full view rows say their column values with
  column names, and the sort state goes into the header buttons' names. Revisit only if users ask for VO table
  navigation.

**The container's role**: drop `role="application"` for `role="group"` (same label). An `application` wrapper around
standard listboxes is the classic anti-pattern, and with focus on the listboxes it no longer needs to claim anything. M0
confirms VoiceOver isn't relying on it.

**Reaching pane chrome without changing `Tab`** (details in M6). Pane chrome is the pane's tab bar, its location bar
(volume chooser, breadcrumb path segments, repo chip, eject), and its sort header. The function key bar is global and
stays as it is.

1. The guard stops bouncing focus that lands on pane chrome (`[data-pane-chrome]`), so VoiceOver with "keyboard focus
   follows the VoiceOver cursor" can rest there and `VO-Space` presses work.
2. While focus is in chrome, the pane forwarding in `key-dispatch.ts` and the navigation-key commands step aside, the
   same way they step aside for a text input, so Enter/Space/arrows act on the focused control.
3. A new command, "Move focus to the pane controls" (`pane.focusControls`), in the palette with the default shortcut
   ⌃F5, remappable through the shortcuts registry like any other. It enters the focused pane's location bar as a
   `role="toolbar"` with roving tabindex: ←/→ walk its controls, ↑/↓ hop to the tab bar and sort header, Escape returns
   to the list. `Tab` stays `pane.switch` everywhere and lands in the other pane's list.

## Milestones

### M0: Spike in the real app (go/no-go, nothing merged)

- **Scope**: a throwaway branch with the minimum to hear the model: unique row ids, `focus({ preventScroll: true })` on
  the focused pane's listbox in `switchPane` and after init, `aria-multiselectable` + marked `aria-selected`,
  `aria-posinset`/`aria-setsize`, a kind suffix, and in `Menu.svelte` an id on submenu rows plus focus moving to the
  submenu surface. Listen with the harness in "Listening harness" below.
- **Intentions**: prove the chosen model reads rows, position, kind, and marked state in Cmdr's WKWebView, and that the
  submenu reads, before building six milestones on it. Also check whether VoiceOver reads `aria-posinset` on a listbox
  option at all (some WebKit versions drop it), what it says for the `..` option that carries NO posinset/setsize while
  its siblings do (it must not invent its own "1 of n"), and what it says for `aria-selected` under multiselect. And
  click a row in the UNFOCUSED pane with a REAL click (System Events `click at {x, y}`; a synthetic click skips the
  mousedown focus default): if VoiceOver first reads the old pane's cursor row (the transient bounce described in M3),
  note it. Also run the real-key scroll check (harness step 6) on the spike, since it's the one regression no E2E can
  see.
- **Landmines**: record macOS, Cmdr, and VoiceOver versions with each finding. `content of last phrase` holds only the
  LATEST phrase: when VoiceOver splits an announcement ("list box" then the row), the poll can miss the first half;
  trust axprobe's notification trace for "what was posted" and the phrase log for "what got spoken".
- **Test plan**: the "What VoiceOver must say" scenarios L1–L5, P1–P2, and U1–U3 below, captured verbatim (phrase log
  plus `axprobe watch` trace per scenario).
- **DONE**: findings appended to this plan (a "Spike results" section) with versions and date; go, or switch to the
  container fallback and re-plan M3.

### M1: Characterization tests (no product change)

- **Scope**: pin CURRENT keyboard and focus behavior, as their own commit.
  - `key-dispatch.test.ts`: the guard bounces a `<button>` inside the container (exists), a path-segment button, a tab,
    a sort header button; keydown forwarding goes to the focused pane whatever the event target is.
  - `global-keydown.test.ts`: Enter, Space, arrows, Home/End, `Tab`, `⇧Tab` resolve the same with `activeElement` on the
    container vs. on a `<button>` inside it (today: identical). M6 deliberately changes the button case; mark those pins
    for it.
  - `Menu.svelte.test.ts`: `→` opens and enters the submenu, ↑/↓ walk it, `←` leaves, Enter picks, a `keepsMenuOpen`
    pick leaves the menu open with focus on a live element (not `<body>`), Escape closes the submenu first, and
    `restoreFocus` runs once on close. Most exist; add the gaps.
  - A new E2E spec (`keyboard-focus.spec.ts`), each asserting WHICH pane's cursor moved on the next ↓: after launch;
    after `Tab` and `Tab` again; after toggling Brief/Full view; after F2 rename commit and cancel; after a dialog (F7
    new folder) closes; after ⌥F1 then Escape; after ⌥F1, `Tab` (chooser moves panes), Escape; after the Favorites menu
    closes; after the pane shows the network view and comes back; after a toast close (exists in `app.spec.ts`, reuse);
    after `⌘F` search results replace a pane's list and close; after the command palette runs "Switch pane" (and,
    separately, closes with Escape); after a mouse click on a row in the unfocused pane; after the Ask Cmdr rail returns
    focus with Escape. Extend `focus-trap.spec.ts` / `app.spec.ts` where a flow already lives there rather than
    duplicating it.
  - The scroll invariant (a key nobody claims never scrolls a pane list) is NOT pinnable in E2E: every
    `tauriPage.keyboard.press` is a synthetic `dispatchEvent` that triggers no browser default and isn't cancelable
    (`test/e2e-playwright/DETAILS.md` § "Keyboard: no press in this suite is a real one"). Pin it with real keys
    instead: harness step 6, run once now on unchanged `main` as the baseline (record the numbers in "Spike results").
  - Test infra: a shared `assertFocusModel(root)` helper, one pure module importable from Vitest and from Playwright
    (run in the page through `evaluate`), so the same contract is checked in jsdom and in WKWebView. It asserts:
    `document.activeElement` is inside `root` and owns `aria-activedescendant` (or names no descendant only when its
    list is empty); the id resolves to exactly ONE rendered element in the document, with a role of `option` /
    `menuitem*` and a non-empty accessible name; every live region the root declares exists at mount (a region inserted
    later announces nothing). Names come from `dom-accessibility-api`'s `computeAccessibleName` (MIT,
    eps1lon/dom-accessibility-api, not archived; latest 0.7.1 published 2025-11-27, checked 2026-10-15; the repo already
    has 0.5.16 transitively through `@testing-library/dom`, so add 0.7.1 as a direct dev dep and re-verify the latest is
    ≥3 days old at install). Add its options for "is the cursor row named" and "focus may be the container" so M1 can
    use it against TODAY's model (container focus, no assertion on activedescendant) and M2–M4 tighten it per widget.
    Unit-test the helper itself (a passing fixture, and one failing fixture per rule).
- **Intentions**: make "keyboard behavior is frozen" provable. Pin reality, not this plan; if something surprises you,
  pin it and flag it.
- **Landmines**:
  - E2E pane-focus helpers already tolerate focus anywhere inside the container; the new spec must assert cursor
    movement, not `activeElement`, so it survives M3 untouched. Use `[data-filename]` and `.is-under-cursor`, never
    `aria-selected` (M4 changes its meaning).
  - Synthetic keys are dispatched on `document.activeElement`. That's what makes these pins catch a focus LOSS (a key
    dispatched on `<body>` never reaches the container's `onkeydown`), so don't "fix" a red pin by dispatching on the
    container.
  - Open dialogs with `dispatchMenuCommand` (`docs/testing.md` decision table); the pin is about where focus lands on
    close. The Escape-closes-menu flows ARE the keyboard path under test, so they may `keyboard.press('Escape')` with a
    comment saying so (precedent: `type-to-jump.spec.ts`); every other cleanup uses `dismissOverlay`.
  - A synthetic click doesn't run the mousedown focus default, so the "click a row in the unfocused pane" pin covers the
    cursor outcome only; the real-click focus transient is M0/M5's job.
- **Test plan**: each new pin fails when you break the behavior it pins (flip one assertion locally, see red).
- **DONE**: `pnpm check` green; the new spec green in `desktop-e2e-playwright` (macOS WKWebView, the target engine) and
  `desktop-e2e-linux`.

### M2: Menus speak, submenus included (#267) and the volume chooser is a button

- **Scope**: `Menu.svelte`, `menu-controller.svelte.ts`, `VolumeBreadcrumb.svelte`, `focus-watchdog.ts`, the four
  focus-restoring consumers only if needed.
  1. Submenu rows get ids from the same `rowId` namespace with a distinct segment (a submenu child's value can equal a
     top-level value). Swap the module counter for `$props.id()` while here (M3 item 1 uses the same).
  2. The submenu surface gets `tabindex="-1"`, `aria-activedescendant` = the submenu cursor's id, and an `aria-label` of
     its parent row's label. The parent row gets `aria-controls` = the submenu surface id while open.
  3. Focus follows the cursor between the two surfaces: into the submenu when `submenuHighlightedValue` becomes
     non-null, back to the parent surface when it goes null or the submenu closes. Do it in `Menu.svelte` (it owns the
     DOM) keyed on the submenu NODE, not the flag (the Ark `Portal` tick race, `ui/DETAILS.md` § Menu gotchas), AND on
     its position: the surface stays `visibility: hidden` until `submenuPosition` lands, and `focus()` on a hidden
     element silently does nothing (M0). The parent surface's open effect (`fitToViewport().then(() => el.focus())`)
     re-runs when the submenu opens and must NOT refocus the parent while focus sits in the submenu (M0: it stole focus
     straight back). A keyboard-opened all-disabled submenu (cursorless by design) takes focus too, with its surface's
     `aria-activedescendant` naming its first row (VoiceOver reads it "dimmed"); the controller's cursor stays null, so
     no key behavior changes.
  4. `focus-watchdog.ts`: add `[data-menu-submenu]` to `KEYBOARD_HOME_SELECTOR`, or focus in a submenu logs a false
     "focus left both panes".
  5. The volume chip becomes `<button type="button" aria-haspopup="menu" aria-expanded={open}>` with an accessible name
     ("{volume name}, choose a volume" or similar), styled to look identical. Same for `.breadcrumb-options-trigger` and
     the popup item (a `menuitem` or plain `button`).
  6. Defer the active descendant one frame after every focus move: when a surface (top-level on open, submenu on entry)
     takes focus, set its `aria-activedescendant` on the next animation frame, not in the same tick. VoiceOver reads
     only "menu Volume switcher" / "menu naspi on …" when focus and the attribute land together, and reads the row
     ("Macintosh HD APFS current location menu item, …") when the attribute follows (M0). One small shared helper (it's
     M3's too), unit-tested with fake timers.
  7. Test infra: add `@guidepup/virtual-screen-reader` as a dev dep (MIT, guidepup/virtual-screen-reader, not archived;
     latest 0.33.0 published 2026-09-20, checked 2026-10-15; re-verify latest ≥3 days old at install, and run
     `pnpm check third-party-notices` plus the `osv-scanner` lane, the repo's JS license/vuln gates). Use it in a
     colocated `Menu.vsr.test.ts` (jsdom) for speech-level expectations: open, ↓, → into a submenu, ↓, ←, Escape, each
     asserting the spoken phrase contains the row name, role, and checked state. It models the ARIA spec, not WebKit or
     VoiceOver, so it guards regressions in our markup; the harness stays the truth for real speech.
- **Intentions**: V10 passes for submenus; the chooser is reachable and announced as a popup button.
- **Landmines**:
  - Focus must leave the submenu surface BEFORE it unmounts, or it drops to `<body>` and the next key goes nowhere
    useful. The `keepsMenuOpen` path (eject, rename) closes only the submenu; pinned in M1. So does the submenu's
    `onmouseleave` → `closeSubmenu()`: the pointer drifting off a keyboard-entered submenu unmounts the focused node.
    Route every close through one path that refocuses the parent surface first. M0 saw the wrong order (unmount, then
    refocus) post a transient focus on the web area on ← and Escape.
  - A hover-opened, cursorless submenu must NOT take focus (it's "shown", not "entered").
  - `isOwnControl` (`Menu.svelte` ~255) treats `<button>` as a row's own control; the chip isn't in a row, but check the
    switcher's `keepOpenWithin` / outside-pointer logic still treats the chip as the anchor after the tag change.
  - Mouse clicks on a `<button>` don't move focus in WKWebView, but a programmatic `.focus()` does: the guard bounces it
    today; after M6 it won't. Nothing here should focus the chip.
  - Button UA styles (padding, font, background, focus ring) must not change the chip's look. David QAs visuals.
- **Test plan**: unit tests for each new attribute and for focus moving in and out (including the unmount path, the
  hidden-until-positioned submenu, and the parent not stealing focus back); `assertFocusModel` on the menu surface and
  the entered submenu; the `Menu.vsr.test.ts` speech expectations; `overlays.a11y.test.ts` and `navigation.a11y.test.ts`
  axe-clean with a submenu open (both are merged files auditing a shared jsdom document and menus portal to `body`: keep
  their `afterEach` body reset, `docs/testing.md` § "Merged files audit a shared jsdom document"); favorites E2E
  unchanged. Then U1–U3 through the harness (cheap now, and it catches a WebKit surprise before M3 builds on the same
  model).
- **DONE**: M1 pins green untouched; `pnpm check` green; `ui/DETAILS.md` § Menu "Focus" bullet rewritten and the ⚠️ "not
  yet verified" gotcha replaced with the result.
- **Status (2026-10-15)**: code, unit, axe, and `Menu.vsr.test.ts` landed. The U1–U4 listening run did NOT happen: the
  Mac's screen was locked (`CGSSessionScreenIsLocked` true; VoiceOver said "Authentication application", the dev window
  couldn't come frontmost), so `ui/DETAILS.md` cites the spike's phrases and says the shipped code's run is owed. Run
  U1–U4 first thing in M5 (or before M3 if the screen is unlocked sooner).

### M3: Focus lives on the focused pane's list (#437 items 1, 6 partly)

- **Scope**: `DualPaneExplorer.svelte`, `key-dispatch.ts`, `FilePane.svelte` + `FilePaneAPI` (`pane/types.ts`),
  `BriefList.svelte`, `FullList.svelte`, `SearchResultsView.svelte`, `pane-access.ts`, `routes/(main)/+page.svelte`
  (scroll-key suppression), the restorers listed in "Current-code facts", `ask-cmdr-trigger.svelte.ts`,
  `NetworkMountView.svelte`.
  1. Unique row ids: a per-instance prefix from `$props.id()` (the house idiom: `QueueRow.svelte:75`,
     `CounterTokenEditor.svelte:35`), e.g. `${uid}-row-42`, the same mechanism M2 gives `Menu.svelte`.
  2. Each list exposes `focusList(): boolean` (`el.focus({ preventScroll: true })`, false when unmounted).
     `FilePaneAPI.focusList()` forwards to whatever list the pane shows, false for non-list views.
  3. ONE seam in `DualPaneExplorer`: `focusActivePane()` tries the focused pane's list and falls back to the container.
     Every `containerElement?.focus()` listed above becomes it; `PaneAccess.focusContainer` is renamed to
     `focusActivePane` everywhere (it no longer focuses the container; no alias). The two selector-based callers
     (`ask-cmdr-trigger.svelte.ts:117`, `NetworkMountView.svelte:262`) move to it too.
  4. The guard, three cases: (a) focus on the focused pane's list stays; (b) focus on the CONTAINER itself is redirected
     to `focusActivePane()` when the focused pane has a list (so every container focuser, including E2E helpers and
     anything missed above, converges on the list; no loop, since the redirect lands on case a); (c) anything else not
     exempt (including the OTHER pane's list or `.full-list` scroller, which a mousedown on its rows focuses) is bounced
     to `focusActivePane()`. A click on a ROW in the unfocused pane produces no transient: the row's own `mousedown`
     handler switches the store before the browser's focus default runs (M0, real click). A click on EMPTY list space
     may still bounce through the old pane's list; check it in M5, and if VoiceOver speaks it, move the store switch to
     a pane-level `mousedown` (capture).
  5. Re-home focus when the list it sits on goes away (Brief/Full toggle, a pane swapping to a network/error/loading
     view, search results opening or closing): when `activeElement` is `<body>` or the container and the focused pane
     has a list, focus it. An `$effect` keyed on the focused pane's list node is the likely shape. ❗ Mandatory, not
     polish: in M0 a view toggle remounted the list, focus fell to `<body>`, and ↓ did nothing until `Tab`.
  6. Focus restore: every restorer in "Current-code facts" (palette, `ModalDialog`, onboarding, toast, the four menu
     consumers) goes through one shared helper, e.g. `restoreFocus(el)` in `lib/ui/`: when `el` is inside the explorer
     it calls the explorer's refocus (registered once by `DualPaneExplorer`), else `el.focus()`. Two reasons, both new
     with this model: a bare `.focus()` on a list lacks `preventScroll` (rule 4), and the remembered list may belong to
     the pane that is no longer focused (palette "Switch pane", ⌥F1 → `Tab` → Escape), leaving DOM focus and the store
     on different panes. ❗ The helper must not refocus when focus already sits in another open menu or dialog: in M0,
     ⌥F1 then `Tab` re-opened the chooser in the other pane, then the OLD chooser's restorer focused its remembered
     list, the guard redirected that to the focused pane's list, and the new chooser stayed open but unfocused (today
     focus stays on the moved chooser). Pin this flow's focus outcome (`activeElement` inside the open `[data-menu]`).
  7. Native keyboard scrolling: with focus inside a scroller, WebKit scrolls it on any arrow, Page, Home/End, or Space
     that nobody `preventDefault`s, yanking the virtual view off the cursor. Suppress those keys when the target is a
     pane list and the global resolver returned nothing (the IGNORE branch of the document handler, after dispatch), ❌
     not with an early `preventDefault` in `key-dispatch.ts`: that trips `unclaimedDispatchWarning` on every arrow. A
     unit test proves the handler calls `preventDefault` (build the event with `cancelable: true`); harness step 6
     proves WebKit then doesn't scroll. ❗ Mandatory: in M0, ↓ on the last row natively scrolled the focused
     `.full-list` (`scrollTop` 7,481 → 7,485; unchanged code stays at 7,481).
  8. Container: `role="group"`, keep the label and handlers. With `role="application"` VoiceOver appends "File explorer
     web application" when focus first enters the explorer (M0).
  9. Re-announce after focus or rows change, with M2's one-frame-deferred active descendant helper: when a list takes
     focus, and after every navigation commit (the cursor id changes before the new rows render, so WebKit posts focus
     to a dead node and VoiceOver stays SILENT on entering a folder, M0 L3), set `aria-activedescendant` on the next
     frame once the cursor row is rendered. Re-setting it two frames later made VoiceOver read "parent folder (1 of 2)".
- **Intentions**: the focused element is the listbox that carries `aria-activedescendant`; nothing about keys changes.
- **Landmines**:
  - `preventScroll` everywhere (rule 4). A missed one shows as the view jumping to the top on `Tab`.
  - No new focus ring: script focus after a keypress matches `:focus-visible` in WebKit, so `Tab` would draw one around
    the list. `.brief-list`, `.full-list`, and `.listbox-region` already set `outline: none` (BriefList ~1136, FullList
    ~991, ~1012); keep it, and the focused-pane look stays the `is-focused` class as today.
  - In Full view a click on empty scroller space focuses `.full-list` (the scroller, `tabindex="-1"`), not the listbox;
    the guard must send that to `focusActivePane()`.
  - Rename: the editor's `<input>` is exempt and its commit path refocuses through `handleFocus` (542). Confirm the
    click-away path too.
  - Wheel-scrolling the cursor row out of the virtual window drops `aria-activedescendant` (the rendered-only rule) and
    scrolling back restores it, which makes VoiceOver re-read the row. Acceptable; don't "fix" it with a dangling id.
  - The rename dialogs and the archive password dialog mount inside `FilePane`; the dialog exemption stays first.
  - `selection.selectAll` reads `activeElement` for inputs only (`command-handlers/selection-handlers.ts:24`): fine.
  - Deliberate test change: `app.spec.ts:146` asserts `activeElement === .dual-pane-explorer`; it becomes "inside the
    explorer and on the focused pane's list". Nothing else in M1 may change.
  - The focus watchdog needs no change (lists are inside `.dual-pane-explorer`); confirm it stays quiet across the M1
    E2E flows (logs).
- **Test plan**: M1 pins green untouched (plus the one named assertion); new unit tests for `focusActivePane` fallback,
  the guard's three cases, re-homing after a view toggle, the restore helper (explorer target vs. outside target vs.
  focus already in another open menu), the deferred active descendant after a navigation commit, the scroll-key
  suppression (claimed key, unclaimed key, target outside a list, text input); `assertFocusModel` on the explorer after
  launch, `Tab`, a view toggle, and a navigation, in the unit tests and in the M1 E2E spec's flows (tightened from M1's
  container option to "focus on the focused pane's list"); `BriefList.a11y.test.ts` / `full-list.a11y.test.ts` assert
  ids are unique with two lists mounted in ONE block (mount through `installLayoutMock()` / `mountFullList()`, or the
  lists render no rows and the check passes vacuously, `docs/testing.md` § "A component that measures itself, rendering
  nothing"). Harness step 6 (real-key scroll) green.
- **Commits**: three, each green on its own: (a) unique ids, `focusList`, `focusActivePane`, the guard, the container
  role, the deferred active descendant; (b) the restore helper and every restorer moved to it, plus re-homing; (c)
  scroll-key suppression. (a) alone already puts focus in a scroller and on lists that remount, so don't merge (a)
  without (b) and (c).
- **DONE**: `pnpm check --include-slow` green (it runs both E2E lanes).

### M4: The list says what it holds (#437 items 2–6)

- **Scope**: `BriefList.svelte`, `FullList.svelte`, `FullListHeader.svelte`, the Brief sort `SortableHeader`s,
  `FilePane.svelte`, a new pane announcer component (item 4), `fileExplorer.json`, `FileEntry` (`types.ts` and its Rust
  source, for `isPackage`). Rebase onto #442 first and extend its row-name helper. 0. ONE shared row-a11y helper in
  `views/file-list-utils.ts` returns the option's whole attribute set (`id`, `aria-label`, `aria-selected`,
  `aria-posinset`, `aria-setsize`) and both lists spread it; the rendered-only activedescendant logic duplicated today
  (BriefList 470-480, FullList 514-523) moves beside it as one function. ❌ No per-view copies of any item below.
  1. Listbox: `aria-multiselectable="true"`; every option `aria-selected={isMarked}` (true or false, always present);
     the cursor stays `aria-activedescendant` only.
  2. Every rendered option, `..` included: `aria-posinset={globalIndex + 1}`, `aria-setsize={totalCount}`. So vo-test
     reads "parent folder, 1 of 8", "subfolder, folder, 2 of 8". Why `..` counts: without `aria-posinset`, VoiceOver
     invents a position from the RENDERED siblings ("parent folder (1 of 8)" beside "subfolder, folder (1 of 7)", M0),
     and under virtualization that count would be wrong too. Every option must carry both attributes.
  3. Row accessible name from one pure helper (unit-tested): Brief = "{name}, {kind}"; Full = "{name}, {kind}, {size},
     modified {date}" (size omitted when unknown, git status when the column shows). Kind: "folder", "symlink",
     "package", "archive"; plain files get no kind word (the extension is in the name). The `..` row's whole name is
     "parent folder". Badge meaning from #442 appends after the kind. Set it as `aria-label` on the option; the visible
     cells stay as they are. "package" needs a typed `isPackage` on `FileEntry`, set in Rust from `is_package_dir`
     (smart backend); ❌ never sniff the `pkg:` icon-id prefix.
  4. Pane announcements: VoiceOver never speaks the listbox's name, neither at launch nor on `Tab`, even with distinct
     names and with the active descendant held back up to 1.2 s (M0). So "which pane, which folder" (#437 item 6, "mark
     the active pane") goes through ONE always-mounted polite live region per pane (`aria-live="polite"`, visually
     hidden, rendered with the pane so it exists before its first message; a region inserted on demand announces
     nothing). It announces "{Left|Right} pane, {folder name}" (catalog string with a `select` on side; the volume name
     at a root; "Search results" when results replace the list) on pane switch and on folder change. Build it as a
     reusable pane announcer, not a one-off: a small component plus a `announce(pane, message)` API owned by `FilePane`,
     so #438 extends it for the empty-folder, loading, selection-count, and filter-count messages without a second
     region. Repeat-safe: clear, then set on the next frame, so the same text announces twice in a row. The listbox
     keeps the same "{Left|Right} pane, {folder}" as its `aria-label` for VO-cursor browsing; the region keeps "Left
     file pane". ❌ No "active" word in any name; it would go stale on the unfocused pane. A live region in M0 spoke
     ("Right pane, vo-target"), but the phrase log couldn't show whether it cut off the cursor row; M5 checks.
  5. Sort state in the sort buttons' names: "Size, sorted descending" / "Name, not sorted" (catalog strings), since
     `aria-sort` needs a `columnheader`.
- **Intentions**: V1 and V3 pass: name, kind, position, and marked state as the cursor moves.
- **Landmines**:
  - `aria-selected` stops meaning "cursor". Grep tests and E2E for it on file rows before changing (none found at
    planning time; `search-helpers.ts` / `server-ops-helpers.ts` hits are other widgets).
  - The label string is built per rendered row per render: keep it a plain function of row data, no `$derived` per row,
    no DOM reads (`ColumnList` rule in `ui/CLAUDE.md` applies in spirit). Reuse the size and date strings the visible
    cells already format; don't format twice. Cost is bounded by the virtual window, never by directory size.
  - On a huge directory `totalCount` grows while the listing streams in, rewriting `aria-setsize` on every rendered row
    each batch. M0/M5: check VoiceOver doesn't re-announce the cursor row on that churn; if it does, hold setsize until
    the listing completes.
  - Brief view's ←/→ move by column; posinset is still linear (`globalIndex`), which is right.
  - The announcer and the cursor row compete for the same moment on `Tab` and on entering a folder. Announce AFTER the
    deferred active descendant lands (M3 item 9) so the row is read first; polite queues behind it. Don't announce on
    every listing batch, only when the folder or the pane changes.
  - The rename `InlineRenameEditor` lives inside the option; an `aria-label` on the option doesn't hide the input, but
    axe `nested-interactive` may complain. Check the a11y tests with rename active.
- **Test plan**: helper unit tests (every kind, `..`, unknown size, git column on/off); posinset/setsize with and
  without a `..` row (every option carries both); announcer unit tests (mounted before its first message, pane switch,
  folder change, same message twice, no message on a listing batch); `BriefList.a11y.test.ts` and
  `full-list.a11y.test.ts` gain marked/unmarked, posinset/setsize, two-lists-mounted cases and `assertFocusModel` (the
  cursor option resolves uniquely and is named "{name}, {kind}"); a `file-list.vsr.test.ts` with
  `@guidepup/virtual-screen-reader` (added in M2) for speech-level expectations: ↓ speaks "subfolder, folder", position
  "2 of 8", marking adds "selected", and the pane announcer's text arrives; axe clean; `accessibility.spec.ts`
  main-window audit clean.
- **DONE**: `pnpm check` green.

### M5: VoiceOver verification of M2–M4

- **Scope**: run every L, P, and U scenario in "What VoiceOver must say" through the listening harness, plus harness
  step 6 and a real click on the unfocused pane (on a row AND on empty list space); fix what fails. U4 includes the two
  menus M0 didn't run: the drive-index badge menu and the viewer's right-click menu. Check the pane announcer against
  the cursor row on `Tab`, launch, and entering a folder: both must be heard, row first. If the announcement cuts the
  row off (or the reverse), record the exact phrases and keys for David rather than tuning timing blind. Anything the
  harness can't settle (a phrase VoiceOver speaks too fast to capture, wording judgment) goes to David as a short list
  with exact repro keys.
- **DONE**: each scenario marked pass with the captured phrase, versions, and date in the audit doc.

### M6: Pane chrome reachable without touching `Tab` (#437 items 7–8)

- **Scope**: `key-dispatch.ts`, `global-keydown.ts`, `FilePane.svelte` header, `TabBar.svelte`, the sort headers, a new
  command in `lib/commands/sources/main-window.ts` (+ handler, palette entry, registry parity test).
  1. `data-pane-chrome` on the location bar, tab bar, and sort header (the tab bar renders in `DualPaneExplorer`, not
     `FilePane`). The guard leaves focus there. Not the function key bar.
  2. While focus is in pane chrome: `key-dispatch` doesn't forward to the pane or run type-to-jump, and
     `resolveGlobalKeyAction` leaves Enter, Space, arrows, Home/End to the control (a predicate beside
     `isTextInputFocused`, single-sourced). `Tab` / `⇧Tab` and every modifier shortcut still dispatch. Escape returns
     focus to the list.
  3. The location bar is a `role="toolbar"` with roving tabindex (volume button, path segments, repo chip, eject); ↑/↓
     hop between the tab bar, location bar, and sort header of the same pane.
  4. The command "Move focus to the pane controls" (`pane.focusControls`), in the palette, default shortcut ⌃F5 (free in
     the registry today), remappable like any other. Check WKWebView receives ⌃F5 while macOS's own "Move focus to the
     window toolbar" (same chord, on with Full Keyboard Access) is enabled; if the system eats it, keep the default and
     say so in the shortcut's description.
- **Intentions**: Full Keyboard Access and VoiceOver users reach every pane control; nobody else notices anything.
- **Landmines**: this is where M1's `global-keydown` pins for "button focused" deliberately change; only those. A click
  on a chrome button must not leave focus there for mouse users (WKWebView doesn't focus buttons on click; verify in the
  app, not jsdom). The type-to-jump intercept must not eat letters while a chrome button is focused.
- **Test plan**: unit tests for the predicate, guard, and roving; E2E: command → focus on the volume button → → → a path
  segment → Enter navigates → focus back in the list; `Tab` from chrome switches panes.
- **DONE**: `pnpm check --include-slow` green (both E2E lanes), VoiceOver scenarios C1–C3 pass through the harness, and
  the real-key check that ⌃F5 reaches the webview (harness step 5) recorded with the Full Keyboard Access state.

### M7: Paperwork and close-out

- `docs/notes/accessibility-audit-2026-10.md`: findings 1 and 8 (lines ~52-66), the 1.1.1/1.3.1/2.4.6/4.1.2 rows (~91,
  ~163-167, ~209), 508 302.1/502.2.2/502.3.x (~251-301), EN 11.5.2.x (~383-385), the V1/V2/V3/V10 entries (~402-421).
  Evidence anchors with versions and dates.
- `apps/website/src/lib/accessibility-report.ts`: rows don't cite issue numbers; find them by their `pending` arrays
  (`grep -nE "pending:.*'V(1|2|3|10)'"`, 10 rows at planning time). Follow the file's header rule: drop each check this
  effort verified from `pending`; only when `pending` is then EMPTY, set `conformance`, rewrite `remark` from what
  VoiceOver did, and drop `codeReview`. Rows still waiting on #438's V6/V7 (or 302.1 and 4.2.1, which wait on all
  twelve) keep their conformance; their remark changes only if it states something now false. Bump `report.date`.
  Remarks are buyer-facing (David reviews them).
- `apps/desktop/src/lib/ui/DETAILS.md` § Menu: "Focus" bullet (~1030) and the ⚠️ gotcha (~1137) replaced with the
  result, macOS and Cmdr versions, date.
- `lib/file-explorer/pane/DETAILS.md` (the focus guard section ~1594, plus a short "Focus model" section),
  `pane/CLAUDE.md` guardrail one-liner (focus lives on the focused pane's list; ❌ never on a row), `views/DETAILS.md`
  (ids, multiselect, posinset, row names), `docs/architecture.md` only if it mentions the old model.
- `test/e2e-playwright/DETAILS.md`: one line where pane focus is described, if it still says focus sits on the
  container.
- Draft comments on #437 and #267 with the results (David approves before posting); close them when M5's scenarios and
  M6's C1–C3 pass.
- Delete this plan and its line in `docs/specs/DETAILS.md` § "The specs" (open work lives in issues).

## Spike results (M0, 2026-10-15)

Environment: macOS 27.0, VoiceOver 10, Cmdr 0.52.0 dev build (`pnpm dev --worktree a11y-gaps`), worktree HEAD
`bf49861b2`, VoiceOver running, app frontmost, real keys posted by `axprobe --key` / System Events. Fixture as in the
harness (`vo-test`, `vo-target`, `vo-long`), plus `vo-p2` (seven `right-r1.txt`…`right-r7.txt`) for P2. Raw phrase logs
and AX traces: the session scratchpad's `m0/logs/` (`base-*` and `spike-*`), with the `m0/h.sh` driver that wraps the
harness steps.

**The spike** (reverted, nothing committed): per-list `$props.id()` row ids, `aria-multiselectable`, `aria-selected` =
marked, `aria-posinset`/`aria-setsize` (none on `..`), an `aria-label` of "{name}, {kind}" ("parent folder" for `..`), a
`focusActivePane()` that focuses the focused pane's `[role="listbox"]` with `preventScroll` (replacing every
`containerElement?.focus()`), the guard letting the focused pane's listbox keep focus and redirecting everything else
(container included) to it, and in `Menu.svelte` submenu row ids, a focusable submenu surface with its own
`aria-activedescendant` and the parent row's label, and an effect moving focus into it while it has a cursor.
`role="application"` stayed on the container.

**Verdict: GO** on the chosen model. Rows, kind, position, and marked state all speak; Tab moves focus between the
panes' lists; submenus read. The fallback (activedescendant on the container) isn't needed. Five findings change M2–M4
(below).

### Baseline (unchanged code) vs. spike, verbatim

- **L1, launch**: baseline "main File explorer web application"; AX focus is the `AXGroup "File explorer"`. Spike
  "parent folder (1 of 8) File explorer web application"; AX focus is the option `#c1-row-0`. The list's NAME is never
  spoken (see finding 2).
- **L2, ↓**: baseline SILENT; only `AXSelectedChildrenChanged` on `AXList "File list"` is posted. Spike: "subfolder,
  folder (1 of 7)", "a.txt (2 of 7)", "b.txt (3 of 7)"; each step posts one `AXFocusedUIElementChanged` with
  `AXARIAPosInSet`/`AXARIASetSize` 1/7, 2/7, 3/7. WebKit honors `aria-posinset` on options. Options surface as
  `AXStaticText` carrying the `aria-label` as `AXTitle`.
- **L3, Enter on subfolder**: spike SILENT on entering; AX posts a focus change to a dead element (`?`), then nothing
  (finding 3). Backspace back: "subfolder, folder (1 of 7)" (it reads, because the cursor id changed after the rows
  existed).
- **L4, marking**: Space on b.txt "b.txt selected (3 of 7)"; ↑ onto marked a.txt "a.txt selected (2 of 7)"; unmarked
  rows say nothing about selection; ⌘A re-reads the cursor row "a.txt selected (2 of 7)", ↓ "b.txt selected (3 of 7)";
  ⌘⇧A re-reads "b.txt (3 of 7)". Kinds: "c.zip, archive (4 of 7)" (symlink and folder likewise).
- **L5, Full view**: "b.txt (3 of 7)", "a.txt (2 of 7)", identical behavior to Brief (sizes/dates are M4's job).
- **P1, Tab**: baseline SILENT (no AX notification at all). Spike: "parent folder (1 of 1)" on the empty right pane, Tab
  again "c.zip, archive (4 of 7)", the left cursor row unchanged. No list name and no side (finding 2).
- **P2**: with the right pane on `vo-p2`, ↓ reads "right-r1.txt (1 of 7)", "right-r2.txt (2 of 7)", ids `c2-row-*`. Id
  collision fixed.
- **U1, ⌥F1**: baseline and spike identical: "menu Volume switcher" only; the highlighted row is NOT read until ↓
  ("naspi on 100.127.48.122 collapsed submenu Volumes group (2 of 4)"). One baseline run lost the menu right after
  opening (not reproduced).
- **U2, → ↓ ↓ ←**: baseline: "naspi on 100.127.48.122 expanded submenu", then SILENT on every ↓ (no notification), ← "…
  collapsed submenu". Spike (after the fix in finding 4): → "menu naspi on 100.127.48.122" (submenu named after its row,
  first row not read), ↓ "Connect directly now selected menu item (2 of 4) menu naspi on 100.127.48.122", ↓ "Use Cmdr's
  fast direct connection selected checked menu item (4 of 4)…", ↑ "Eject (naspi on 100.127.48.122) selected menu item (1
  of 4)", ← "naspi on 100.127.48.122 collapsed submenu". The separator counts in "of 4" (three rows).
- **U3**: Escape closes the submenu ("… collapsed submenu"), Escape again "parent folder (1 of 8) File explorer web
  application" (back on the list's cursor row). ⌥F1, Tab, Escape: focus ends on the other pane's list, but see
  finding 5.
- **U4** (partial): Favorites (⌃D) "menu Favorites menu", ↓ "Desktop collapsed submenu Favorites group (1 of 2)", Escape
  back to the cursor row. Archive Enter popup "menu Open archive or bundle", ↓ "Open with default app menu item", Escape
  "c.zip, archive (4 of 7) File explorer web application". Not run: drive-index badge menu, viewer menu.
- **Real click on the unfocused pane** (System Events `click at`, trusted `mousedown` confirmed): no transient. The
  row's own `mousedown` handler switches the store before the browser's focus default runs, so the only notification is
  the clicked row ("right-r4.txt (4 of 7)"). The M3 item 4 "move the switch to pane `mousedown`" fallback isn't needed
  for row clicks; a click on empty list space is untested.
- **Harness step 6 (real-key scroll)**, `vo-long`, baseline then spike: Brief view identical (no-op keys leave
  `scrollLeft`/`scrollTop` at 0 and 1,162; Home, ↓ × 30, Page Down lands on `file-181.txt`, `scrollLeft` 206, in view).
  Full view: ↓ × 30 + Page Down identical (`file-057.txt`, `scrollTop` 621, in view); Space on `..` and mid-list is
  claimed (no scroll). REGRESSION: ↓ on the last row natively scrolls the focused `.full-list` (End lands at 7,481, ↓
  takes it to the 7,485 max; baseline stays at 7,481). M3 item 7 is needed, as planned. Full view's → and ← move the
  cursor (End/Home), so they aren't "no-op" keys there.

### Findings, and where the plan absorbed them

1. **`..` without posinset gets an INVENTED position.** VoiceOver reads it "parent folder (1 of 8)" (or "(1 of 2)", "(1
   of 1)"): it counts the RENDERED sibling options when `aria-posinset` is absent. Leaving `..` uncounted therefore
   makes "parent folder (1 of 8)" sit next to "subfolder, folder (1 of 7)". Setting posinset/setsize to 0 on `..` was
   inconclusive (the spread re-applied attributes mid-test). Decided: `..` counts (M4 item 2), pending David's review
   (Open questions).
2. **The listbox name is never spoken**, neither at launch nor on Tab (tried with distinct names "Left pane, vo-test" /
   "Right pane, vo-target", and with `aria-activedescendant` held back 120 ms and 1.2 s after focus: WebKit posts the
   `AXList` focus, VoiceOver stays silent until the row). So the original assumption that focus alone marks the active
   pane (VoiceOver reading the list name on every switch) was false. A polite live region set on pane switch DID speak
   ("Right pane, vo-target"), but `content of last phrase` can't tell whether it cut off the row phrase. Decided: a
   per-pane polite live region announces pane and folder (M4 item 4); M5 checks the cut-off.
3. **Re-announce after the rows change under focus.** Entering a folder (L3) is silent: the new cursor id is set before
   the new rows render, so WebKit posts focus to a dead node and never again. Removing `aria-activedescendant` and
   re-setting it two frames later made VoiceOver read "parent folder (1 of 2)". The same trick fixes menus: on ⌥F1 the
   highlighted row isn't read, but re-setting the surface's `aria-activedescendant` after focus read "Macintosh HD APFS
   current location menu item, group Volumes group (2 of 4)". Absorbed: M2 item 6 (menus, the shared helper) and M3 item
   9 (lists, after focus and every navigation commit).
4. **Submenu focus needs two guards (absorbed in M2 item 3 and its landmines).** (a) The submenu surface starts
   `visibility: hidden` until positioned, and `focus()` on it silently fails: key the effect on `submenuPosition` too.
   (b) The parent surface's open effect (`fitToViewport().then(() => el.focus())`) re-runs when the submenu opens and
   steals focus back; it must skip when focus is inside the submenu. With both, focus moved in. Also confirmed the M2
   landmine: on ← and Escape the submenu unmounts BEFORE the parent refocus, so AX posts a transient `AXWebArea` focus
   (body); VoiceOver still read the parent row, but route closes through "refocus parent, then unmount".
5. **Focus restorers race with a re-opened menu (absorbed in M3 item 6).** ⌥F1 then Tab: baseline keeps focus on the
   moved chooser ("menu Volume switcher", AX focus stays `AXMenu`). Spike: the moved chooser takes focus, then 250 ms
   later focus lands on the right list ("parent folder…"), so the chooser is open but unfocused (keys still work via the
   capture listener). Cause: the closing chooser's restorer focuses its remembered element (the LEFT list), the guard
   redirects to the focused pane's list. The shared restore helper must not refocus when focus already sits in another
   open menu.
6. **A list remount drops focus to `<body>`** and pane keys die (↓ did nothing until Tab). Seen after an MCP view-mode
   toggle (Full → Brief) with the spike. Absorbed: M3 item 5 is mandatory, and so is item 7 (the Full view ↓ scroll
   regression above); M3's commits ship all three together.
7. **`role="application"` stays audible**: with it, VoiceOver appends "File explorer web application" when focus first
   enters the explorer. M3 item 8 (`role="group"`) should remove that; untested in the spike, so M5 confirms.

Harness notes: `content of last phrase` misses phrases that VoiceOver replaces within ~150 ms (some L2/L4 steps), so the
AX trace is the source of truth for "posted". A reload of the page (`location.reload()` through the tauri MCP bridge)
stands in for launch; HMR from other agents' edits can leave `getExplorer()` null ("Explorer is not ready") until a
reload, and after `git restore` a reload can still run the old modules once (check the row ids).

## Listening harness (M0, M2, M5, M6)

VoiceOver's AppleScript bridge works on David's Mac (verified 2026-10-15): `content of last phrase` returns the last
spoken text. The helpers sit in the planning session's scratchpad,
`/private/tmp/claude-501/-Users-veszelovszki-projects-git-vdavid-cmdr/adf428f5-420d-42d1-bfdb-68342d6dd352/scratchpad/vo/`:
`vo-last.sh` (`poll <secs>` prints each new phrase with a timestamp) and `axprobe` (`trusted`, `focus`, `watch`, `tree`,
`get`, `press`; `--key <keycode> --mods cmd,shift,opt,ctrl` posts a real key to the pid first). Copy them to your own
scratchpad before starting (`/tmp` gets pruned); if they're gone, rebuild with `swiftc -O axprobe.swift -o axprobe`.
Agents run them in the foreground; only the dev app runs in the background.

1. **Launch.** `pgrep -f 'worktrees/a11y-gaps/target/debug/Cmdr'` must be empty (else drive the running one: a second
   launch on the same data dir is refused with a desktop alert, `apps/desktop/DETAILS.md`). From the WORKTREE dir, start
   `pnpm dev --worktree a11y-gaps` in the background (Vite must serve this tree's code), then probe until
   `CMDR_INSTANCE_ID=dev-a11y-gaps ./scripts/mcp-call.sh --list-tools` answers (the `dev-` prefix is required). The data
   dir is fresh, so the first launch shows the onboarding wizard: close it once and confirm both panes render.
   `PID=$(pgrep -f 'worktrees/a11y-gaps/target/debug/Cmdr')`.
2. **Fixture**, in your scratchpad (❌ not `~/Desktop`, `~/Documents`, or `~/Downloads`: a fresh dev binary gets a TCC
   prompt there): `vo-test/` = `subfolder/` (one file inside), `a.txt` (1 KB), `b.txt`, `c.zip` (a real zip), `d-link`
   (symlink to `a.txt`), `e.bin` (100 MB, `mkfile -n`), `f.txt`; `vo-target/` empty; `vo-long/` with 400 small files.
   Navigate with `./scripts/mcp-call.sh nav_to_path '{"pane":"left","path":"…/vo-test"}'` (and right → `vo-target`),
   then put focus back in the left pane with a real key or click, not MCP.
3. **Frontmost + VoiceOver on.**
   `osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $PID) to true"`.
   VoiceOver must already be running (David toggles it; ❌ don't send ⌘F5). Check `axprobe trusted` and that
   `vo-last.sh` returns text. Record `sw_vers -productVersion`,
   `defaults read /System/Library/CoreServices/VoiceOver.app/Contents/Info CFBundleShortVersionString`, and
   `git rev-parse --short HEAD`.
4. **One scenario step** = start `vo-last.sh poll 3 > phrases.log &` and `axprobe watch $PID 3 --key <code> [--mods …]`
   in the same foreground command, then `wait`. The phrase log says what was spoken; the watch trace says what WebKit
   posted (focus change, activedescendant/selection change) and is the deterministic half. Hints ("You are currently
   on…") arrive a beat later as their own phrase: ignore them. Key codes: ↓ 125, ↑ 126, ← 123, → 124, Tab 48, Return 36,
   Escape 53, Space 49, Backspace 51, Home 115, End 119, Page Up 116, Page Down 121, F1 122, F2 120, F5 96, F7 98, A 0.
   So ⌥F1 is `--key 122 --mods opt`, ⌃F5 `--key 96 --mods ctrl`, ⌘A `--key 0 --mods cmd`. If an F-key does nothing,
   retry with System Events (`key code 122 using option down`) before concluding the app ignored it.
5. **⌃F5 reaches the webview** (M6): record `defaults read NSGlobalDomain AppleKeyboardUIMode` (Full Keyboard Access
   state), send ⌃F5, and check the watch trace shows focus moving to the volume button.
6. **Real-key scroll check.** In `vo-long`, Brief then Full view: read the focused pane's scroller `scrollTop`
   (`document.querySelector('.file-pane.is-focused .brief-list, .file-pane.is-focused .full-list').scrollTop` through
   the tauri MCP's `webview_execute_js`), press with real keys each navigation key that doesn't move the cursor in that
   state (↑ and Home on the first row, End and ↓ on the last, ← / → in Full view, Space on `..`), and assert it didn't
   change; then ↓ × 30 and Page Down and assert the cursor row stays inside the viewport. Record the numbers.

## What VoiceOver must say

VoiceOver's exact wording and order vary by version, so a scenario passes when every listed piece is spoken and nothing
stale is. Role words may differ from what's written here ("list box" or "list", "pop-up button" or "menu button");
names, kinds, positions, and "selected" may not. Scenario → David's guide: L* and P2 are V1, L4 is V3, P1 is V2, U* is
V10. Setup: the harness fixture (`vo-test` with seven entries, `vo-target` empty, `subfolder/` holding one file), left
pane in `vo-test`, right in `vo-target`.

Positions count the `..` row (M4 item 2), so vo-test's set size is 8. At launch the cursor is on `..` (the fixture isn't
a volume root). The listbox's own name is never spoken; the pane announcer (M4 item 4) carries "{side} pane, {folder}"
after the cursor row.

- **L1, launch** (focus lands in the left list): the cursor row ("parent folder, 1 of 8"), then the announcer "Left
  pane, vo-test". ↓: "subfolder, folder, 2 of 8".
- **L2, ↓ ↓**: each step speaks only the new row: "a.txt, 3 of 8", "b.txt, 4 of 8". Nothing from the right pane.
- **L3, Enter on the subfolder, then Backspace**: the new cursor row ("parent folder, 1 of 2"; M0 heard NOTHING here
  before the deferred active descendant), then the announcer "Left pane, subfolder"; Backspace: "subfolder, folder, 2 of
  8" and "Left pane, vo-test".
- **L4, Space on a.txt, ↓, Space on b.txt, ↑**: marking says "selected" on that row; moving onto a marked row says
  "selected"; an unmarked row says nothing about selection (or "not selected"). ⌘A then moving: every row "selected".
  ⌘⇧A: none.
- **L5, Full view (switch view), ↓**: "a.txt, 1 KB, modified 2026-10-14 …, 3 of 8"; on a folder "subfolder, folder, …".
  The view toggle keeps focus in the list (no silent `<body>`). Sort buttons (when reached through chrome): "Name,
  sorted ascending, button".
- **P1, Tab**: the right pane's cursor row ("parent folder, 1 of 1" in the empty `vo-target`), then the announcer "Right
  pane, vo-target"; `Tab` again: the left cursor row unchanged, then "Left pane, vo-test". Both pieces heard, row first;
  a cut-off goes to David (M5).
- **P2, the left pane's own ids**: with both panes on folders of the same size, ↓ in the RIGHT pane reads right-pane
  names (regression check for the id collision).
- **U1, ⌥F1**: the menu's label ("Volume switcher"), "menu", then the highlighted volume ("Macintosh HD APFS current
  location menu item", read thanks to the deferred active descendant; M0 heard only "menu Volume switcher"); ↓ reads
  each row.
- **U2, → on a row with a submenu, ↓ ↓, ←**: entering says the submenu (labelled after its row, "menu naspi on …") and
  its first row ("Eject (naspi on …)"); each ↓ reads the next submenu row (toggles say "checked"/"unchecked"); ← returns
  and reads the parent row again.
- **U4, every other `Menu` consumer** (#267 names them all): the Favorites menu, the archive Enter popup, the
  drive-index badge menu, and the viewer's right-click menu each open with their menu label and read the highlighted
  row, and ↓ reads the next one. M0 ran Favorites and the archive popup only.
- **U3, Escape twice**: the submenu closes (parent row read), then the menu closes and VoiceOver is back on the list's
  cursor row in the same pane. ⌥F1, `Tab`: the moved chooser keeps focus (its menu label is read, not a list row);
  Escape: back on the OTHER pane's cursor row.
- **C1, the volume chip** (VO-→ to it from the pane region): "Macintosh HD, choose a volume, pop-up button" (or "menu
  button"); VO-Space opens the menu as in M1.
- **C2, ⌃F5 (the pane controls command)**: focus lands on the volume button; → reads each path segment as a button; ↑
  reaches the tab bar, ↓ the sort header; Enter on a segment navigates and VoiceOver is back on the list; Escape from
  anywhere in the controls returns to the list's cursor row.
- **C3, VO cursor onto a path segment with "keyboard focus follows VoiceOver cursor" on**: focus stays there (no jump
  back to the list); VO-Space navigates.

## Invariants

1. Every key does what it did, in the same pane, with the same cursor (M1 pins, green and untouched except the named
   deliberate changes in M3 and M6).
2. DOM focus is never on a file row. It's on the focused pane's list, an input, dialog content, a menu surface, pane
   chrome (only when the user put it there), or the container as the fallback.
3. Every `aria-activedescendant` names a RENDERED element with a document-unique id, on the element that holds focus,
   and is (re)set one frame after that element takes focus or its rows change.
4. `aria-selected` on a file row means marked; the cursor is only `aria-activedescendant` and the `is-under-cursor`
   class.
5. `Tab` is `pane.switch` everywhere in the main window, chrome included.
6. Every `.focus()` on a list passes `preventScroll: true`, and no focus restorer calls a bare `.focus()` on an explorer
   element (they go through the shared restore helper).
7. Focus leaves an element before that element unmounts (submenu, list on a view toggle), or is re-homed the moment it
   lands on `<body>`.
8. axe stays clean in every a11y test and in `accessibility.spec.ts`.
9. A key nobody claims never scrolls a pane list natively; the virtual scroller alone moves the view.

## Open questions for David

Decided: the pane controls command defaults to ⌃F5 (remappable) and covers the tab bar, location bar, and sort header,
not the function key bar; plain files carry no kind word; `..` is "parent folder"; agents listen through the AppleScript
bridge (enabled); the `assertFocusModel` helper and `@guidepup/virtual-screen-reader` join the test infra.

Decided by the lead after M0, for David's review:

- **`..` counts in positions** ("parent folder, 1 of 8", "subfolder, folder, 2 of 8"). This reverses the earlier
  "outside the count" decision because VoiceOver invents a position for an option without one, so leaving `..` out made
  it say "1 of 8" next to rows saying "1 of 7".
- **Pane and folder are announced through a per-pane polite live region**, since VoiceOver never reads the list's name.
  M5 reports whether it cuts off the cursor row.

1. **VoiceOver on during runs**: the harness needs VoiceOver running and Cmdr frontmost for each listening session (M0,
   after M2, M5, M6), so the Mac is busy for those minutes. Fine to run unattended, or do you want a heads-up?
