# Accessibility audit, 2026-10 (VPAT 2.5Rev INT evidence)

The criterion-by-criterion evidence behind Cmdr's accessibility conformance report. It follows the ITI VPAT 2.5Rev INT
edition (April 2025): WCAG 2.2 Level A and AA (applied to software through W3C's WCAG2ICT), Revised Section 508
(chapters 3, 5, and 6), and EN 301 549 (clauses 4, 5, 11, and 12). The criteria list and wording come from the
downloaded INT `.docx` (itic.org, `VPAT2.5RevINT`).

## Scope and method

- **Product**: Cmdr desktop app for macOS (`apps/desktop/`), Svelte 5 frontend in a Tauri 2 WKWebView, at `main`
  `f31f2aec5` (2026-10-14). The app is "open functionality software": it exposes its UI through WebKit's bridge to the
  macOS accessibility API (NSAccessibility), so VoiceOver and other assistive tech can reach it. Documentation and
  support: getcmdr.com (`apps/website/`), the in-app Keyboard shortcuts window, email, and in-app feedback.
- **Methods**: code reading of every surface (file panes, dialogs, toasts, forms, settings, search, viewer, queue);
  contrast math on the theme tokens in light and dark mode under all nine accent variants; the check lanes
  `a11y-contrast`, `a11y-coverage`, and `svelte-tests` (all green on 2026-10-14, `pnpm check -m`, 3m36s).
- **Automated coverage that exists**: 110 `*.a11y.test.ts` files with about 920 axe-core test cases (tags `wcag2a`,
  `wcag2aa`, `wcag21a`, `wcag21aa`, `wcag22aa`, `best-practice`; `apps/desktop/src/lib/test-a11y.ts`); the
  `a11y-coverage` gate (every `lib/**/*.svelte` has a test or a reasoned allowlist entry); the deterministic contrast
  checker `scripts/check-a11y-contrast/` (WCAG 2.2 plus an APCA Lc 45 floor, accent matrix, row-state matrix); and 11
  E2E axe tests in light and dark mode (12 since #442 added the queue window, which also gained a tier-3 file,
  `routes/queue/queue.a11y.test.ts`) (`apps/desktop/test/e2e-playwright/accessibility.spec.ts`, `color-contrast` off
  there because the static checker owns it). `docs/testing.md` still says 696 tests; the count above is from the files.
- **Not done**: no screen reader run. Rows that depend on what VoiceOver actually says are marked "Pending VoiceOver
  check" with the check ID (V1–V12) from David's guide (vault, `projects/Cmdr/2026-10-14 Cmdr VoiceOver test guide.md`).
  Live UI wasn't judged from screenshots.
- **Levels**: Supports, Partially supports, Does not support, Not applicable (the VPAT's own terms). "Partially
  supports" means some functionality doesn't meet the criterion.

## Summary

Counts per table (rows marked "see WCAG section" aren't counted):

- **WCAG 2.2 Level A (32)**: 19 Supports, 7 Partially supports, 0 Does not support, 6 Not applicable (3.3.1 moved to
  Supports with #438, 2026-10-15).
- **WCAG 2.2 Level AA (24)**: 9 Supports, 10 Partially supports, 0 Does not support, 5 Not applicable (1.3.5 and 3.1.2
  moved to Supports with #442, 2026-10-15).
- **Section 508 chapter 3 (9)**: 4 Supports, 5 Partially supports.
- **Section 508 chapter 5 (26)**: 9 Supports, 7 Partially supports, 10 Not applicable (502.3.6 moved to Supports with
  #438, 2026-10-15).
- **Section 508 chapter 6 (5)**: 2 Supports, 2 Partially supports, 1 Not applicable.
- **EN 301 549 clause 4 (11)**: 6 Supports, 5 Partially supports.
- **EN 301 549 clause 5 (11)**: 2 Supports, 9 Not applicable.
- **EN 301 549 clause 11 (23)**: 8 Supports, 7 Partially supports, 8 Not applicable (11.5.2.8 moved to Supports with
  #438, 2026-10-15).
- **EN 301 549 clause 12 (5)**: 3 Supports, 2 Partially supports.
- **Whole chapters/clauses not applicable**: 508 chapter 4 (hardware); EN 301 549 clauses 6 (two-way voice), 7 (video),
  8 (hardware), 9 (web: the app isn't web content; the website is assessed as documentation under 12.1.2), 10 (non-web
  documents: none ship), and 13 (relay and emergency services). WCAG AAA (Table 3) wasn't evaluated: none of the three
  standards requires it.

**Risk of downgrade**: if V1 confirms the file list is silent under VoiceOver, 4.1.2, 1.3.1, 508 302.1 and 502.3.12, and
EN 4.2.1 and 11.5.2.13 should drop to "Does not support", since browsing files is the product's core.

Top gaps, each with its issue:

1. **The file list likely doesn't speak** (#437): the focused element is the `role="application"` container, while
   `aria-activedescendant` sits on the never-focused listbox; marked files, list position, file kind, and Full view
   columns aren't exposed.
2. **Cursors and status changes aren't announced** (#438, closed in code 2026-10-15, VoiceOver checks pending): the
   Search result cursor, the Settings sidebar, empty folders, selection counts, the license key error, progress text.
3. **Toasts are mouse-first** (#439): focus doesn't pause them, no key reaches them, no setting keeps them, and Undo
   trash exists only in a 10-second toast.
4. **State cues rely on faint color** (#440): which pane is focused (about 1.09:1), marked rows (hue only), focus rings
   under 3:1 with yellow, orange, green, or Cmdr gold accents in light mode, field borders 1.36:1.
5. **Pointer and keyboard leftovers** (#441): tab reorder is drag-only, sub-24px targets, tooltips not hoverable, viewer
   bare-key shortcuts not remappable, zoom tops out at 150%.
6. **Quick fixes** (#442, closed in code 2026-10-15): Reduce motion misses, hard-coded English labels and window titles,
   a missing `lang`, a missing `autocomplete`, badges without text, no queue window a11y test.
7. **No accessibility documentation** (#443): nothing lists the features, limits, or how to get help.
8. Already tracked: in-app menus under VoiceOver (#267).

Strengths a buyer can rely on: everything core runs from the keyboard (every action is a command with a rebindable
shortcut and a command palette entry); all dialogs share one primitive with correct roles, focus trap, Escape, and focus
restore; text contrast is gated in CI across both themes and every macOS accent; macOS text size, dark mode, accent
color, Reduce motion, and Reduce transparency are honored; no audio, video, flashing, or time limits beyond toasts;
native menu bar and context menus.

## WCAG 2.2 Level A

- **1.1.1 Non-text content**: Partially supports. Icon buttons are named, the viewer image uses the file name as alt
  text, and the sync and symlink badges name themselves into the row's accessible name (#442, 2026-10-15), but file-kind
  icons are `alt=""`, so folders and files sound the same. `Icon` is `aria-hidden` unless given a label. Evidence:
  `lib/file-explorer/selection/FileIcon.svelte` (`alt=""` at the kind icon; badge `aria-label` / `alt`);
  `lib/ui/Icon.svelte`; `routes/viewer/MediaImageView.svelte` (`alt={fileName}`); spot checks of `ModalDialog.svelte`,
  `TabBar.svelte`, `SettingRow.svelte` aria-labels. #437.
- **1.2.1 Audio-only and video-only (prerecorded)**: Not applicable. Cmdr has no audio or video content; the viewer
  shows text, images, and PDFs only. Evidence: no `<audio>`/`<video>` in `apps/desktop/src`;
  `routes/viewer/media-view.ts`.
- **1.2.2 Captions (prerecorded)**: Not applicable. Same as 1.2.1.
- **1.2.3 Audio description or media alternative (prerecorded)**: Not applicable. Same as 1.2.1.
- **1.3.1 Info and relationships**: Partially supports. Dialogs, settings, and server forms use real headings, labels,
  and roles. The file list doesn't expose marked state, list position, or Full view column structure, and sort order is
  visual only. The Settings sidebar nests each section's subsections in a `role="group"` named for it (#438,
  2026-10-15). Evidence: `views/BriefList.svelte`, `views/FullList.svelte` (`aria-selected` = cursor only, no
  `aria-setsize`/`aria-posinset`), `views/FullListHeader.svelte`, `selection/SortableHeader.svelte` (caret
  `aria-hidden`, no `aria-sort`), `settings/components/SettingsSidebar.svelte`. Pending VoiceOver check V1, V7. #437.
- **1.3.2 Meaningful sequence**: Supports. DOM order matches the visual order in every surface; virtualized rows render
  in list order. Evidence: code reading of panes, dialogs, settings. Confirm with V12.
- **1.3.3 Sensory characteristics**: Supports. Instructions don't rely on shape, position, or sound alone; pane regions
  are named "Left file pane" and "Right file pane". Evidence: `pane/FilePane.svelte` region label; English catalog
  review.
- **1.4.1 Use of color**: Partially supports. Git status uses letters, errors use labeled glyphs, and size/age colors
  are decorative. Marked files differ from normal rows mostly by red hue, hidden files only by a dimmer color, and the
  focused pane only by a faint fill. Evidence: `app.css` `--color-selection-fg-primary`, `app-file-list.css` marked-row
  rules, `--color-text-quiet`; `git/status-column.ts`. #440.
- **1.4.2 Audio control**: Not applicable. Cmdr plays no audio.
- **2.1.1 Keyboard**: Partially supports. Every core action is a command with a shortcut and a command palette entry:
  pane switch, navigation, marking, open, go up, go to path, volume chooser, tabs, sort, divider, view mode, all file
  operations, settings. Gaps: reordering tabs or moving a tab to the other pane needs a drag, and toast actions
  (including Undo trash) can't be reached from the keyboard. Evidence: `lib/commands/sources/*.ts`,
  `tabs/tab-drag-controller.svelte.ts`, `ui/toast/ToastItem.svelte`, `delete/TrashCompleteToastContent.svelte`. #439,
  #441.
- **2.1.2 No keyboard trap**: Supports. Dialog focus traps always release on Escape or their buttons; nested traps
  stack. Evidence: `lib/ui/focus-trap.ts`, `ModalDialog.svelte`, lint `cmdr/dialog-needs-focus-trap`.
- **2.1.4 Character key shortcuts**: Partially supports. File-list single keys (`*`, `+`, `-`) are rebindable and active
  only with list focus; type-to-jump only acts with pane focus. The viewer's single-key shortcuts (`W`, `F`, `0`–`3`,
  `+`/`=`/`-`) work window-wide and can't be remapped or turned off. Evidence: `shortcuts/shortcuts-store.ts`,
  `pane/type-to-jump-keys.ts`, `routes/viewer/viewer-keyboard.ts`, `viewer-view-mode.ts`, `MediaImageView.svelte`. #441.
- **2.2.1 Timing adjustable**: Partially supports. The only time limits are toasts (4 s default, 7–10 s for some). Hover
  pauses them; keyboard focus doesn't, and no setting extends them. The quit-while-busy 15 s countdown falls under the
  essential exception (macOS logout deadline). Evidence: `ui/toast/toast-store.svelte.ts`, `ToastItem.svelte`,
  `lib/quit/QuitConfirmationDialog.svelte`. #439.
- **2.2.2 Pause, stop, hide**: Supports. Moving content is limited to activity indicators (spinners, shimmer, pulses),
  which stop under Reduce motion; the transfer tree's auto-follow pauses on user intent. No marquee or auto-updating
  feed. Evidence: `app-utilities.css`, `ui/ProgressBar.svelte`, `file-operations/transfer-tree/`.
- **2.3.1 Three flashes or below threshold**: Supports. Only single 300 ms warning flashes and 1 Hz caret blinks.
  Evidence: `VolumeSpaceLine.svelte`, `VolumeChooserMenu.svelte`, `ViewerTextCursor.svelte`.
- **2.4.1 Bypass blocks**: Not applicable. Applies to web pages and sets of software; Cmdr is a single app (EN 11.2.4.1
  and 508 don't apply to software).
- **2.4.2 Page titled**: Supports. Each window has a title: main "Cmdr", the viewer the file name, plus Settings,
  Keyboard shortcuts, Queue, Debug. Every user-facing title comes from the catalog (Settings and Keyboard shortcuts
  since #442, 2026-10-15); Debug is a dev-build-only window and stays English like the rest of it. A language switch
  while a child window is open keeps its OS title until it reopens (no `set-title` capability there). EN and 508 don't
  apply this to software. Evidence: `src-tauri/src/licensing/app_status.rs`, `routes/viewer/+page.svelte`,
  `settings-window.ts`, `shortcuts-window.ts`, `debug-window.ts`, `queue-window.ts`.
- **2.4.3 Focus order**: Supports. Dialogs move focus in on open (child autofocus, first field, or the dialog), trap Tab
  in order, and restore focus to the trigger on close; the command palette does the same. Evidence: `ModalDialog.svelte`
  (records and restores `document.activeElement`), `focus-trap.ts`, `CommandPalette.svelte`.
- **2.4.4 Link purpose (in context)**: Supports. In-app links and buttons name their target; no "click here" copy.
  Evidence: English catalog grep (`lib/intl/messages/en/*.json`).
- **2.5.1 Pointer gestures**: Supports. Swipe and mouse side buttons for back/forward have ⌘[ / ⌘] and menu equivalents;
  viewer pinch and wheel zoom have `+`/`-`/`0`. Evidence: `src-tauri/src/mouse_nav.rs`, `commands/sources/file-list.ts`,
  `MediaImageView.svelte`.
- **2.5.2 Pointer cancellation**: Supports. Row `mousedown` only moves the cursor, selects, or arms a drag; opening is
  on double-click and buttons act on click. Evidence: `FullList.svelte`, `BriefList.svelte` row handlers,
  `FunctionKeyBar.svelte`.
- **2.5.3 Label in name**: Supports. Five spot checks of visibly labeled buttons all start their accessible name with
  the visible text (Tail, Search, Dismiss, Pause, Remove). Evidence: `ViewerToolbar.svelte`, `QueryBar.svelte`,
  `QueueRow.svelte`, `TransferProgressDialog.svelte`, `MediaIndexChosenFolders.svelte`.
- **2.5.4 Motion actuation**: Not applicable. No device-motion input.
- **3.1.1 Language of page**: Supports. Every window sets `<html lang>` from the chosen UI language (11 languages).
  Evidence: `intl/messages.svelte.ts` `setLocale()`, `intl/document-language.ts`, `initWindowLanguageSync()` in each
  window route.
- **3.2.1 On focus**: Supports. No control changes context on focus. Evidence: code reading of panes, dialogs, settings.
- **3.2.2 On input**: Supports. Settings apply live (a change of setting, not context); the Settings sidebar switches
  its content pane on arrow keys, like tabs. Evidence: `SettingsSidebar.svelte`.
- **3.2.6 Consistent help**: Supports. The native Help menu sits in the same place in every window (Keyboard shortcuts,
  What's new, Send feedback, debug log, error report). Evidence: `src-tauri/src/menu/menu_bar.rs`.
- **3.3.1 Error identification**: Supports. Rename, new folder, server, and license key forms mark errors with
  `aria-invalid` and tie the message to the field; the license key's activation error also lands in an always-mounted
  `role="alert"` region (#438, 2026-10-15). Evidence: `InlineRenameEditor.svelte`, `NewEntryNameField.svelte`,
  `ServerFormFields.svelte`, `licensing/LicenseKeyDialog.svelte` (`showError`, `licensing.a11y.test.ts`). Pending
  VoiceOver check V5.
- **3.3.2 Labels or instructions**: Supports. Settings and server forms use visible `<label for>`; the license key,
  rename, and new folder fields have a visible prompt plus an accessible name. Evidence: `SettingRow.svelte`,
  `ServerFormFields.svelte`, `SignInCredentialFields.svelte`, `LicenseKeyDialog.svelte`.
- **3.3.7 Redundant entry**: Supports. The contact email is stored and prefilled for feedback, error reports, and
  onboarding; server credentials stay in the Keychain. Evidence: `attach-email/attach-email.svelte.ts`.
- **4.1.1 Parsing**: Supports. Obsolete in WCAG 2.2 and always satisfied; the DOM is generated by Svelte.
- **4.1.2 Name, role, value**: Partially supports. Dialogs, settings controls (Ark UI-based checkbox, switch, select,
  slider), tabs, the command palette, the queue grid, and menus expose name, role, and state. The file list's cursor
  isn't conveyed (activedescendant on an unfocused element), marked state isn't exposed, and the volume chooser trigger
  isn't a control. The Search result cursor and the Settings sidebar selection are conveyed as `aria-activedescendant`
  on the focused field or list, landing a frame after focus (#438, 2026-10-15). Evidence: `pane/key-dispatch.ts`
  `handleFocusGuard`, `pane/DualPaneExplorer.svelte` (`role="application"`), `query-ui/QueryBar.svelte`,
  `SettingsSidebar.svelte` (both with virtual-screen-reader tests), `navigation/VolumeBreadcrumb.svelte`. Pending
  VoiceOver check V1, V3, V6, V7, V10. #437, #267.

## WCAG 2.2 Level AA

- **1.2.4 Captions (live)**: Not applicable. No live media.
- **1.2.5 Audio description (prerecorded)**: Not applicable. No video.
- **1.3.4 Orientation**: Supports. Desktop windows don't lock orientation.
- **1.3.5 Identify input purpose**: Supports. Server sign-in fields carry `autocomplete="username"` and
  `current-password`, and all three email fields (attach-email, onboarding, Settings > Updates) `email` (the last two
  since #442, 2026-10-15). Evidence: `ServerFormFields.svelte`, `SignInCredentialFields.svelte`,
  `AttachEmailCheckbox.svelte`, `onboarding/StepBeta.svelte`, `settings/sections/UpdatesSection.svelte`.
- **1.4.3 Contrast (minimum)**: Partially supports. Every text token pair is gated at 4.5:1 (3:1 large) in CI in light
  and dark mode under all nine accent variants and all pane tints; marked file names measure 4.52:1 worst case (light)
  and 4.71:1 (dark). Exceptions: with macOS Increase contrast on, light mode can fall to about 4.18:1 on a brown pane
  tint (computed; the checker doesn't model those tints yet), and the opt-in "App" date and size palettes put the raw
  accent on text (gold about 2.4:1). Core tokens on `#ffffff` / `#1e1e1e`: primary 17.40 / 13.61, secondary 5.74 / 9.56,
  tertiary 5.74 / 7.18, error text 6.47 / 8.78, warning text 7.31 / 9.89. Evidence: `pnpm check a11y-contrast` (green),
  `scripts/check-a11y-contrast/row_state_matrix.go`, `app.css` `prefers-contrast: more` block. #440.
- **1.4.4 Resize text**: Supports. App zoom (⌘+ / ⌘−, 75–150%) compounds with the macOS Accessibility text size
  (0.82–3.12×), so text reaches 200% and beyond; tokens are rem-based and rows grow with text. App zoom alone stops at
  150% (#441). Evidence: `src-tauri/src/text_size.rs`, `lib/text-size.svelte.ts`, `command-handlers/view-handlers.ts`,
  `settings/reactive-settings.svelte.ts`.
- **1.4.5 Images of text**: Supports. All text is live text; no images of text in the UI.
- **1.4.10 Reflow**: Partially supports. Dialogs and Settings are single-column and their minimum widths scale with
  text. The main window has a 950×550 minimum and a dual-pane, columnar layout that needs two dimensions (the exception
  for content that requires 2-D layout covers the file grid, not the chrome). Pending a manual check at 150% zoom plus
  large system text. Evidence: `src-tauri/tauri.conf.json`, `settings/settings-window.ts`, `file-viewer/open-viewer.ts`.
- **1.4.11 Non-text contrast**: Partially supports. Checkbox and switch borders use `--color-control-border` (about
  3.7:1). Text field borders are 1.36:1 (light) and 1.71:1 (dark); the 2px accent focus ring is under 3:1 on white with
  Apple Yellow (1.58), Cmdr gold (2.38), Green (2.41), and Orange (2.58); the focused-pane cue is about 1.09:1.
  Evidence: `app-field.css` (`--color-border`), `app.css` `button:focus-visible`, `--shadow-focus-solid`,
  `app-file-list.css` cursor rules; ratios computed from `scripts/check-a11y-contrast/accent_matrix.go` hexes. #440.
- **1.4.12 Text spacing**: Partially supports. Rows have fixed pixel heights with `overflow: hidden`; a 1.5 line height
  fits the Comfortable density but clips in Compact (16 px rows), and middle truncation measures with canvas metrics
  that ignore letter and word spacing overrides. Pending a manual text-spacing test. Evidence: `BriefList.svelte`,
  `FullList.svelte` row styles, `settings/types.ts` density mappings.
- **1.4.13 Content on hover or focus**: Partially supports. Tooltips show on hover and keyboard focus, stay until
  pointer leave, blur, or a key, and any key dismisses them. They can't be hovered: `pointer-events: none` and the
  trigger's `mouseleave` hides them. Evidence: `lib/tooltip/tooltip.ts`, `app-tooltip.css`. #441.
- **2.4.5 Multiple ways**: Not applicable. Applies to sets of web pages or software.
- **2.4.6 Headings and labels**: Supports. Settings uses `h1`/`h2`/`h3`, dialogs an `h2` title, panes named regions.
  Both panes' listboxes share the name "File list" (#437). Evidence: `routes/settings/+page.svelte`,
  `SettingsSection.svelte`, `ModalDialog.svelte`, `FilePane.svelte`.
- **2.4.7 Focus visible**: Partially supports. Buttons, links, fields, and menus show a focus ring (`:focus` for fields
  so mouse users see it too), and the file cursor has an accent fill plus outline. Which pane holds focus is close to
  invisible in light mode (focused vs unfocused cursor about 1.09:1 with gold, 1.02:1 with Apple Yellow). Evidence:
  `app.css`, `app-field.css`, `app-file-list.css`. #440.
- **2.4.11 Focus not obscured (minimum)**: Partially supports. Dialogs are modal and centered. Toasts are fixed at the
  top right and can cover the right pane's first rows, cursor included, until they time out. Evidence:
  `ui/toast/ToastContainer.svelte`. #439.
- **2.5.7 Dragging movements**: Partially supports. The pane divider (double-click reset, ⌃⌥←/→), drag-and-drop copy and
  move (F5/F6, context menu, copy and paste) have alternatives; tab reordering and moving a tab to the other pane don't.
  Evidence: `PaneResizer.svelte`, `commands/sources/main-window.ts`, `tabs/tab-drag-controller.svelte.ts`. #441.
- **2.5.8 Target size (minimum)**: Partially supports. Sort headers, function-key bar, and the new-tab button meet 24×24
  px. The tab close button is 14×14 px, breadcrumb segments are about 17 px tall with no padding, and the pane divider's
  hit area is 4 px wide. Evidence: `TabBar.svelte`, `FilePane.svelte` breadcrumb styles, `PaneResizer.svelte`. #441.
- **3.1.2 Language of parts**: Supports. The language picker lists each language in its own name, with a per-option
  `lang` on the row and on the trigger while selected (#442, 2026-10-15). Evidence: `intl/locale-display-names.ts`,
  `settings/definitions/appearance.ts` `languageOptions()`, `ui/Select.svelte`.
- **3.2.3 Consistent navigation**: Not applicable. Applies to sets of web pages or software. (The menu bar is the same
  in every window.)
- **3.2.4 Consistent identification**: Not applicable. Same as 3.2.3. (Commands keep one name everywhere through the
  command registry.)
- **3.3.3 Error suggestion**: Supports. Error copy says what happened and what to try, for example the listing,
  invalid-name, and license-server messages. Evidence: `lib/intl/messages/en/errors.json` `*.suggestion` keys,
  `docs/style-guide.md`.
- **3.3.4 Error prevention (legal, financial, data)**: Supports. Delete always confirms, permanent delete confirms even
  with "skip confirmation" on, overwrites and rename collisions get conflict dialogs, trash is undoable, and copy and
  move can be rolled back. (Undo trash's toast is mouse-only today, #439.) Evidence: `settings/definitions/behavior.ts`,
  `DeleteDialog.svelte`, `TransferConflictDialog`, `RenameConflictDialog.svelte`,
  `operation-log/RollbackControls.svelte`.
- **3.3.8 Accessible authentication (minimum)**: Supports. License key, API keys, and passwords accept paste; no
  cognitive tests. Evidence: no `onpaste` handlers in `apps/desktop/src`; `SettingPasswordInput.svelte`.
- **4.1.3 Status messages**: Partially supports. Toasts use `status`/`alert`, the transfer phase banner and Search
  result counts are polite live regions, listing errors are alerts. Since #438 (2026-10-15), each file pane's one live
  region also says an empty folder, a slow load, the selection count, and the quick filter's match count (debounced, so
  a burst says its end state), and the command palette and the Settings search say their result counts. Gap: the toast
  live region mounts late with alerts nested in a polite region. EN and 508 don't apply this to software. Evidence:
  `ToastContainer.svelte`, `TransferProgressDialog.svelte`, `QueryResults.svelte`, `pane/ErrorPane.svelte`,
  `pane/pane-status-announcer.svelte.ts`, `ui/LiveAnnouncer.svelte`, `CommandPalette.svelte`, `SettingsSidebar.svelte`.
  Pending VoiceOver check V4, V9. #439.

## Revised Section 508

### Chapter 3: functional performance criteria

- **302.1 Without vision**: Partially supports. Dialogs, settings, menus, and forms are built for screen readers; the
  file list probably doesn't announce its cursor or marked files. Pending VoiceOver check V1–V12. #437.
- **302.2 With limited vision**: Partially supports. macOS text size plus app zoom past 200%, dark mode, AA text
  contrast in CI. Weak focused-pane and focus-ring contrast with some accents. #440, #441.
- **302.3 Without perception of color**: Partially supports. Marked files and hidden files rely on color; the rest uses
  glyphs and text. #440.
- **302.4 Without hearing**: Supports. Cmdr uses no audio cues.
- **302.5 With limited hearing**: Supports. Same as 302.4.
- **302.6 Without speech**: Supports. No speech input needed.
- **302.7 With limited manipulation**: Partially supports. Full keyboard operation with rebindable shortcuts and no
  simultaneous-key requirement beyond modifiers (Sticky Keys works). Tab reorder is drag-only, toast actions are
  pointer-only, and some targets are under 24 px. #439, #441.
- **302.8 With limited reach and strength**: Supports. Software only; no physical reach.
- **302.9 With limited language, cognitive, and learning abilities**: Partially supports. Plain, conversational copy
  (`docs/style-guide.md`), confirmations, undo and rollback, consistent menus. Toasts time out before some users can act
  on them. #439.

### Chapter 4: hardware

- Not applicable. Cmdr is software.

### Chapter 5: software

- **501.1 Scope, incorporation of WCAG 2.0 AA**: See the WCAG 2.2 sections.
- **502.2.1 User control of accessibility features**: Not applicable. Cmdr isn't platform software.
- **502.2.2 No disruption of accessibility features**: Supports. VoiceOver's ⌃⌥ chords pass through (type-to-jump bails
  on ⌃/⌥), system zoom isn't blocked, menus are native. Main-window `Tab` is a rebindable pane switch, which keeps Full
  Keyboard Access off the pane chrome (#437). Evidence: `pane/type-to-jump-keys.ts`, `commands/sources/main-window.ts`.
- **502.3.1 Object information**: Partially supports. Standard controls expose name, role, and state through WebKit;
  file rows lack kind and marked state. Pending VoiceOver check V1, V3. #437.
- **502.3.2 Modification of object information**: Supports. States set by the user (checkboxes, switches, selects) are
  standard ARIA controls that assistive tech can change. Pending VoiceOver check V7.
- **502.3.3 Row, column, and headers**: Partially supports. The queue window is a `role="grid"`; the Full view file list
  has no column headers tied to cells. Evidence: `routes/queue/+page.svelte`, `FullListHeader.svelte`. #437.
- **502.3.4 Values**: Supports. Progress bars expose `aria-valuenow/min/max`, and the transfer readout's bars also say
  the amount on screen as `aria-valuetext` ("1.2 GB of 3 GB", #438, 2026-10-15); sliders and number fields expose
  values. Evidence: `ui/ProgressBar.svelte`, `file-operations/TransferProgressReadout.svelte`. Pending VoiceOver check
  V4.
- **502.3.5 Modification of values**: Supports. Sliders, number inputs, and selects are Ark UI controls operable by
  assistive tech. Pending VoiceOver check V7.
- **502.3.6 Label relationships**: Supports. `<label for>` and `aria-describedby` throughout, the license key error and
  the two alertdialogs (`DeleteAiModelDialog`, `ForgetMemoryDialog`) included since #438 (2026-10-15). Pending VoiceOver
  check V5.
- **502.3.7 Hierarchical relationships**: Partially supports. Dialogs, menus, the tablist, and the Settings sidebar
  (subsections grouped under their section, #438) are structured; tabs have no `tabpanel`. #437.
- **502.3.8 Text**: Supports. All UI text is live DOM text exposed through WebKit. Pending VoiceOver check V11.
- **502.3.9 Modification of text**: Supports. Text entry uses native `<input>`/`<textarea>`.
- **502.3.10 List of actions**: Supports. Controls are native buttons and ARIA widgets with standard actions. Pending
  VoiceOver check V8.
- **502.3.11 Actions on objects**: Supports. Same as 502.3.10. Pending VoiceOver check V8.
- **502.3.12 Focus cursor**: Partially supports. Dialog and settings focus is real DOM focus, and the Search cursor and
  Settings sidebar selection are exposed as `aria-activedescendant` (#438, 2026-10-15); the file list cursor isn't.
  Pending VoiceOver check V1, V6. #437.
- **502.3.13 Modification of focus cursor**: Partially supports. Assistive tech can move focus among standard controls;
  moving the VoiceOver cursor onto a file row likely doesn't move Cmdr's cursor. Pending VoiceOver check V1. #437.
- **502.3.14 Event notification**: Partially supports. See WCAG 4.1.3. #439.
- **502.4 Platform accessibility features**: Not applicable. Applies to platforms.
- **503.2 User preferences**: Partially supports. Honors system font, dark/light, accent color, Reduce transparency,
  Reduce motion, and macOS text size. The Reduce motion misses (settings sidebar jump, rename shake and zoom, boot
  loader, switch thumb, disclosure chevrons, swatch and onboarding-dot grows) closed with #442 on 2026-10-15. Increase
  contrast only changes pane tints; Differentiate without color isn't read. Evidence: `app.css` (`--font-system`,
  `prefers-contrast` block), `src-tauri/src/reduce_transparency.rs`, `src-tauri/src/text_size.rs`,
  `lib/accent-color.ts`. #440.
- **503.3 Alternative user interfaces**: Not applicable. Cmdr provides no alternative UI acting as assistive tech.
- **503.4.1 Caption controls**: Not applicable. No video.
- **503.4.2 Audio description controls**: Not applicable. No video.
- **504.2 Content creation or editing**: Not applicable. Cmdr isn't an authoring tool.
- **504.2.1 Preservation of information provided for accessibility in format conversion**: Not applicable.
- **504.2.2 PDF export**: Not applicable.
- **504.3 Prompts**: Not applicable.
- **504.4 Templates**: Not applicable.

### Chapter 6: support documentation and services

- **602.2 Accessibility and compatibility features**: Partially supports. The Keyboard shortcuts window (Help menu)
  documents every keyboard command; nothing documents the other accessibility features or known limits. #443.
- **602.3 Electronic support documentation**: Supports. getcmdr.com passes axe on all 13 pages in light and dark mode
  (the `website-e2e` check), and Lighthouse accessibility scored 100 (2026-09-08). Evidence:
  `apps/website/e2e/accessibility.spec.ts`, `apps/website/.lighthouseci/`.
- **602.4 Alternate formats for non-electronic support documentation**: Not applicable. No non-electronic docs.
- **603.2 Information on accessibility and compatibility features**: Partially supports. Support answers by email
  (hello@getcmdr.com) and in-app feedback, but has no accessibility documentation to point to. #443.
- **603.3 Accommodation of communication needs**: Supports. All support is written (email, in-app feedback); no phone
  needed.

## EN 301 549

### Clause 4: functional performance statements

- **4.2.1 Usage without vision**: Partially supports. See 508 302.1. Pending VoiceOver check V1–V12. #437.
- **4.2.2 Usage with limited vision**: Partially supports. See 508 302.2. #440.
- **4.2.3 Usage without perception of colour**: Partially supports. See 508 302.3. #440.
- **4.2.4 Usage without hearing**: Supports. No audio.
- **4.2.5 Usage with limited hearing**: Supports. No audio.
- **4.2.6 Usage with no or limited vocal capability**: Supports. No speech input.
- **4.2.7 Usage with limited manipulation or strength**: Partially supports. See 508 302.7. #439, #441.
- **4.2.8 Usage with limited reach**: Supports. Software only.
- **4.2.9 Minimize photosensitive seizure triggers**: Supports. See WCAG 2.3.1.
- **4.2.10 Usage with limited cognition, language or learning**: Partially supports. See 508 302.9. #439.
- **4.2.11 Privacy**: Supports. Accessibility features don't expose private data; passwords and keys use masked fields
  and the Keychain.

### Clause 5: generic requirements

- **5.1 Closed functionality (5.1.2–5.1.7)**: Not applicable. Cmdr is open to assistive technology.
- **5.2 Activation of accessibility features**: Supports. Zoom runs from the keyboard, the View menu, and Settings;
  system accessibility settings apply without extra steps.
- **5.3 Biometrics**: Not applicable. No biometric identification.
- **5.4 Preservation of accessibility information during conversion**: Not applicable. Cmdr copies and archives files
  without converting their formats.
- **5.5.1 Means of operation**: Not applicable. Hardware.
- **5.5.2 Operable parts discernibility**: Not applicable. Hardware.
- **5.6.1 Tactile or auditory status**: Not applicable. No locking or toggle keys of its own.
- **5.6.2 Visual status**: Not applicable. Same as 5.6.1.
- **5.7 Key repeat**: Not applicable. Uses the system key repeat.
- **5.8 Double-strike key acceptance**: Not applicable. Uses the system keyboard settings.
- **5.9 Simultaneous user actions**: Supports. Chords are modifier plus key, which Sticky Keys sequences; nothing needs
  two pointer actions at once.

### Clauses 6–10 and 13

- Not applicable. See the summary.

### Clause 11: software

- **11.1–11.4 (WCAG mappings)**: See the WCAG 2.2 sections.
- **11.5.1 Closed functionality**: Not applicable.
- **11.5.2.1–11.5.2.3 Platform accessibility service support and use**: See 11.5.2.5–11.5.2.17. Cmdr uses the platform
  service through WebKit's NSAccessibility bridge.
- **11.5.2.4 Assistive technology**: Not applicable. Cmdr isn't assistive technology.
- **11.5.2.5 Object information**: Partially supports. See 508 502.3.1. #437.
- **11.5.2.6 Row, column, and headers**: Partially supports. See 508 502.3.3. #437.
- **11.5.2.7 Values**: Supports. See 508 502.3.4.
- **11.5.2.8 Label relationships**: Supports. See 508 502.3.6.
- **11.5.2.9 Parent-child relationships**: Partially supports. See 508 502.3.7. #437.
- **11.5.2.10 Text**: Supports. See 508 502.3.8.
- **11.5.2.11 List of available actions**: Supports. See 508 502.3.10.
- **11.5.2.12 Execution of available actions**: Supports. See 508 502.3.11.
- **11.5.2.13 Tracking of focus and selection attributes**: Partially supports. See 508 502.3.12; marked files aren't
  exposed as selected. #437.
- **11.5.2.14 Modification of focus and selection attributes**: Partially supports. See 508 502.3.13. #437.
- **11.5.2.15 Change notification**: Partially supports. See WCAG 4.1.3. #439.
- **11.5.2.16 Modifications of states and properties**: Supports. See 508 502.3.2.
- **11.5.2.17 Modifications of values and text**: Supports. See 508 502.3.5 and 502.3.9.
- **11.6.1 User control of accessibility features**: Not applicable. Cmdr isn't platform software.
- **11.6.2 No disruption of accessibility features**: Supports. See 508 502.2.2.
- **11.7 User preferences**: Partially supports. See 508 503.2. #440.
- **11.8.1–11.8.5 Authoring tools**: Not applicable.

### Clause 12: documentation and support services

- **12.1.1 Accessibility and compatibility features**: Partially supports. See 508 602.2. #443.
- **12.1.2 Accessible documentation**: Supports. See 508 602.3.
- **12.2.2 Information on accessibility and compatibility features**: Partially supports. See 508 603.2. #443.
- **12.2.3 Effective communication**: Supports. See 508 603.3.
- **12.2.4 Accessible documentation**: Supports. See 508 602.3.

## Pending VoiceOver checks

Each ID matches a step in David's guide; the result fills the rows named.

- **V1 Browse a folder**: does VoiceOver read each file name as the cursor moves, with "folder" vs file and "n of
  total"? Rows: 1.1.1, 1.3.1, 4.1.2, 302.1, 502.3.1, 502.3.12, 502.3.13, 4.2.1, 11.5.2.5, 11.5.2.13, 11.5.2.14.
- **V2 Switch panes**: does it say which pane (and path) you're in? Rows: 1.3.1, 2.4.6.
- **V3 Mark files**: does it say "selected" for marked files, and the count? Rows: 4.1.2, 502.3.1, 11.5.2.13.
- **V4 Copy with the progress dialog**: dialog title on open, progress value, completion message. Rows: 4.1.3, 502.3.4,
  502.3.14, 11.5.2.7, 11.5.2.15.
- **V5 Rename**: field name, and an announced error for an invalid name. Rows: 3.3.1, 502.3.6, 11.5.2.8.
- **V6 Search**: result count, and the highlighted result as you arrow. Rows: 4.1.2, 4.1.3, 502.3.12.
- **V7 Settings**: section names in the sidebar, a control's label and value, and the new value after a change. Rows:
  1.3.1, 4.1.2, 502.3.2, 502.3.5, 502.3.7, 11.5.2.16, 11.5.2.17.
- **V8 Confirmation dialog (delete)**: title, message, and buttons; pressing a button works. Rows: 502.3.10, 502.3.11,
  11.5.2.11, 11.5.2.12.
- **V9 Toast or error message**: is it read out without moving focus? Rows: 4.1.3, 11.5.2.15.
- **V10 In-app menus** (volume chooser, favorites): rows read as you arrow. Rows: 4.1.2. Tracked in #267.
- **V11 Read text**: an error pane and a text file in the viewer read in full. Rows: 502.3.8, 11.5.2.10.
- **V12 Reading order**: VoiceOver's VO-arrow order in a dialog and in Settings matches the screen. Rows: 1.3.2.
