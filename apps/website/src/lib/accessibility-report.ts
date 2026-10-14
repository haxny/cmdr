/**
 * The `/trust/accessibility` page's content: Cmdr's accessibility conformance report, based on
 * ITI's VPAT 2.5Rev INT edition (WCAG 2.2 A/AA, Revised Section 508, EN 301 549).
 * `src/pages/trust/accessibility.astro` is a template over this and owns the prose sections.
 *
 * ❗ The evidence behind every row is `docs/notes/accessibility-audit-2026-10.md` (file paths, code
 * references, and contrast math). Remarks here are for buyers: plain, specific, no file paths. When
 * a gap closes, update the row and its remark, and bump `report.date`.
 *
 * Screen-reader checks. David's VoiceOver guide (vault, `projects/Cmdr/2026-10-14 Cmdr VoiceOver
 * test guide.md`) has 12 checks, V1–V12, described in `voiceOverChecks` below. A row that waits on
 * one lists it in `pending`; that array is the single source for "which rows does V3 fill" (search
 * for `'V3'`; 302.1 and 4.2.1 wait on all twelve). The rule for a pending row:
 *   - Code review found it fully supported → `conformance: 'not-evaluated'`, because only the
 *     screen-reader test can confirm it. `codeReview` keeps the code-review result.
 *   - Code review already found gaps → it stays `'partially-supports'`. The test can only confirm it
 *     or lower it (V1 failing would lower the file-list rows to `'does-not-support'`).
 * The page adds "A screen-reader test is pending." after the remark of every row with `pending`, so
 * remarks don't say it themselves. Filling in a result: set `conformance`, rewrite `remark` from
 * what VoiceOver did, drop the check from `pending` (and `codeReview` once `pending` is empty). The
 * page needs no edit.
 *
 * Write straight quotes; `smart-quotes.ts` curls them in the built HTML. Remarks are plain text.
 */

export type Conformance =
  | 'supports'
  | 'partially-supports'
  | 'does-not-support'
  | 'not-applicable'
  | 'not-evaluated'
  /** A row the VPAT answers by pointing elsewhere; `seeLabel` says where. */
  | 'see-other'

export type VoiceOverCheck = 'V1' | 'V2' | 'V3' | 'V4' | 'V5' | 'V6' | 'V7' | 'V8' | 'V9' | 'V10' | 'V11' | 'V12'

const allChecks: VoiceOverCheck[] = ['V1', 'V2', 'V3', 'V4', 'V5', 'V6', 'V7', 'V8', 'V9', 'V10', 'V11', 'V12']

/** Labels for the page's tables, in the VPAT's own terms plus one for pending rows. */
export const conformanceLabel: Record<Conformance, string> = {
  supports: 'Supports',
  'partially-supports': 'Partially supports',
  'does-not-support': 'Does not support',
  'not-applicable': 'Not applicable',
  'not-evaluated': 'Not evaluated yet',
  'see-other': 'See remarks',
}

/** What each term means, shown in the report's "Terms" section. */
export const conformanceTerms: { term: Conformance; meaning: string }[] = [
  {
    term: 'supports',
    meaning:
      'The functionality of the product has at least one method that meets the criterion without known defects, or meets it with equivalent facilitation.',
  },
  { term: 'partially-supports', meaning: "Some functionality of the product doesn't meet the criterion." },
  { term: 'does-not-support', meaning: "The majority of product functionality doesn't meet the criterion." },
  { term: 'not-applicable', meaning: "The criterion isn't relevant to the product." },
  {
    term: 'not-evaluated',
    meaning:
      "The criterion depends on a screen-reader test that hasn't been run yet. This isn't one of the VPAT's terms; it marks rows this report will fill in later.",
  },
]

export const report = {
  product: 'Cmdr for macOS',
  version: '0.52.0',
  /** The evaluation date, also the page's "Last updated". */
  date: '2026-10-15',
  edition: 'VPAT® 2.5Rev INT (April 2025)',
}

export interface CriterionRow {
  /** The criterion's number, like `1.4.3` or `502.3.12`. */
  id: string
  name: string
  /** WCAG level. Only set in the WCAG tables. */
  level?: 'A' | 'AA'
  conformance: Conformance
  /** For a `'see-other'` row: the level column's text, like "See WCAG 2.2 tables". */
  seeLabel?: string
  remark: string
  /** GitHub issues in `vdavid/cmdr` that track this row's gaps. */
  issues?: number[]
  /** Screen-reader checks this row waits on. See the file comment. */
  pending?: VoiceOverCheck[]
  /** For a `'not-evaluated'` row: what the code review found. */
  codeReview?: Conformance
}

export interface ReportTable {
  /** Anchor id on the page. */
  id: string
  title: string
  /** Optional plain-text note under the heading. */
  note?: string
  rows: CriterionRow[]
}

export interface StandardSection {
  id: string
  title: string
  tables: ReportTable[]
}

const wcagA: CriterionRow[] = [
  {
    id: '1.1.1',
    name: 'Non-text content',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Icon buttons have names, the image viewer uses the file name as the image's text alternative, and the sync-status and symlink badges on file icons are named. File-type icons have no text, so a folder and a file sound the same.",
    issues: [437],
    pending: ['V1'],
  },
  {
    id: '1.2.1',
    name: 'Audio-only and video-only (prerecorded)',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'Cmdr has no audio or video content. Its viewer shows text, images, and PDFs.',
  },
  {
    id: '1.2.2',
    name: 'Captions (prerecorded)',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'No audio or video content.',
  },
  {
    id: '1.2.3',
    name: 'Audio description or media alternative (prerecorded)',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'No audio or video content.',
  },
  {
    id: '1.3.1',
    name: 'Info and relationships',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Dialogs, Settings, and server forms use real headings, labels, and roles. The file list doesn't expose which files are marked, each file's position in the list, or the column structure of the Full view. Sort order is shown visually only, and the Settings sidebar doesn't expose its section hierarchy.",
    issues: [437, 438],
    pending: ['V1', 'V2', 'V7'],
  },
  {
    id: '1.3.2',
    name: 'Meaningful sequence',
    level: 'A',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'The reading order in the code matches the visual order in every window, including long file lists.',
    pending: ['V12'],
  },
  {
    id: '1.3.3',
    name: 'Sensory characteristics',
    level: 'A',
    conformance: 'supports',
    remark:
      'Instructions don\'t rely on shape, position, or sound alone. The two panes are named "Left file pane" and "Right file pane".',
  },
  {
    id: '1.4.1',
    name: 'Use of color',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      'Git status uses letters, errors use labeled symbols, and size and date colors are optional decoration. Marked files differ from other files mostly by their red color, hidden files only by a dimmer color, and the focused pane only by a faint background.',
    issues: [440],
  },
  { id: '1.4.2', name: 'Audio control', level: 'A', conformance: 'not-applicable', remark: 'Cmdr plays no audio.' },
  {
    id: '2.1.1',
    name: 'Keyboard',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Every core action has a keyboard shortcut and a command palette entry: switching panes, navigating, marking, opening, every file operation, tabs, sorting, view modes, and Settings. Two gaps: reordering tabs or moving a tab to the other pane needs a mouse drag, and the buttons in notifications (including Undo after moving files to the trash) can't be reached from the keyboard.",
    issues: [439, 441],
  },
  {
    id: '2.1.2',
    name: 'No keyboard trap',
    level: 'A',
    conformance: 'supports',
    remark: 'Dialogs keep focus inside while open and always release it on Escape or their buttons.',
  },
  {
    id: '2.1.4',
    name: 'Character key shortcuts',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Single-key shortcuts in the file list (like * and +) can be remapped and work only while the file list has focus. In the file viewer, single-key shortcuts (W, F, 0–3, + and -) work anywhere in the window and can't be remapped or turned off.",
    issues: [441],
  },
  {
    id: '2.2.1',
    name: 'Timing adjustable',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "The only time limits are notifications, which close after 4–10 seconds. Hovering over one pauses it, but keyboard focus doesn't, and no setting makes them stay longer. The 15-second countdown when quitting during a file operation is essential: it follows the macOS logout deadline.",
    issues: [439],
  },
  {
    id: '2.2.2',
    name: 'Pause, stop, hide',
    level: 'A',
    conformance: 'supports',
    remark:
      'Moving content is limited to activity indicators like spinners, which stop under the macOS Reduce motion setting. The live view of a running copy stops following when the user scrolls.',
  },
  {
    id: '2.3.1',
    name: 'Three flashes or below threshold',
    level: 'A',
    conformance: 'supports',
    remark: "Nothing flashes more than once per second: one short warning flash and the text cursor's blink.",
  },
  {
    id: '2.4.1',
    name: 'Bypass blocks',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'Applies to web pages and sets of software. Cmdr is a single app.',
  },
  {
    id: '2.4.2',
    name: 'Page titled',
    level: 'A',
    conformance: 'supports',
    remark:
      'Every window has a title in the app\'s language: "Cmdr" for the main window, the file name in the viewer, and a name for Settings, Keyboard shortcuts, and the queue.',
  },
  {
    id: '2.4.3',
    name: 'Focus order',
    level: 'A',
    conformance: 'supports',
    remark:
      'Dialogs and the command palette move focus in when they open, keep Tab in a logical order, and return focus to where it was when they close.',
  },
  {
    id: '2.4.4',
    name: 'Link purpose (in context)',
    level: 'A',
    conformance: 'supports',
    remark: 'Links and buttons in the app name where they go or what they do.',
  },
  {
    id: '2.5.1',
    name: 'Pointer gestures',
    level: 'A',
    conformance: 'supports',
    remark:
      'Swipe gestures and mouse side buttons for back and forward have keyboard and menu equivalents. Pinch and scroll-wheel zoom in the viewer have keyboard equivalents.',
  },
  {
    id: '2.5.2',
    name: 'Pointer cancellation',
    level: 'A',
    conformance: 'supports',
    remark:
      'Pressing the mouse button on a file only moves the cursor, selects, or starts a drag. Opening needs a double-click, and buttons act on release.',
  },
  {
    id: '2.5.3',
    name: 'Label in name',
    level: 'A',
    conformance: 'supports',
    remark: 'Buttons with visible text have an accessible name that starts with that text.',
  },
  {
    id: '2.5.4',
    name: 'Motion actuation',
    level: 'A',
    conformance: 'not-applicable',
    remark: 'Cmdr uses no device-motion input.',
  },
  {
    id: '3.1.1',
    name: 'Language of page',
    level: 'A',
    conformance: 'supports',
    remark: 'Every window declares the language the user chose for the app (11 languages).',
  },
  {
    id: '3.2.1',
    name: 'On focus',
    level: 'A',
    conformance: 'supports',
    remark: 'No control changes the context when it gets focus.',
  },
  {
    id: '3.2.2',
    name: 'On input',
    level: 'A',
    conformance: 'supports',
    remark:
      'Settings apply as soon as they change, which changes a setting, not the context. The Settings sidebar switches its section with the arrow keys, like tabs.',
  },
  {
    id: '3.2.6',
    name: 'Consistent help',
    level: 'A',
    conformance: 'supports',
    remark:
      "The macOS Help menu is in the same place in every window: keyboard shortcuts, what's new, feedback, and error reports.",
  },
  {
    id: '3.3.1',
    name: 'Error identification',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Rename, new folder, and server forms mark the field in error and tie the message to it. The license key error isn't tied to its field or announced.",
    issues: [438],
    pending: ['V5'],
  },
  {
    id: '3.3.2',
    name: 'Labels or instructions',
    level: 'A',
    conformance: 'supports',
    remark:
      'Settings and server forms have visible labels. The license key, rename, and new folder fields have a visible prompt and an accessible name.',
  },
  {
    id: '3.3.7',
    name: 'Redundant entry',
    level: 'A',
    conformance: 'supports',
    remark:
      'The contact email is remembered and filled in for feedback and error reports. Server passwords are kept in the macOS Keychain.',
  },
  {
    id: '4.1.1',
    name: 'Parsing',
    level: 'A',
    conformance: 'supports',
    remark: 'WCAG 2.2 treats this criterion as always met.',
  },
  {
    id: '4.1.2',
    name: 'Name, role, value',
    level: 'A',
    conformance: 'partially-supports',
    remark:
      "Dialogs, Settings controls, tabs, the command palette, the queue, and menus expose their name, role, and state. The file list doesn't convey which file the cursor is on or which files are marked, Search results and the Settings sidebar don't convey the highlighted item, and the volume chooser button isn't exposed as a control. If VoiceOver confirms the file list is silent, this row becomes \"Does not support\".",
    issues: [437, 438, 267],
    pending: ['V1', 'V3', 'V6', 'V7', 'V10'],
  },
]

const wcagAA: CriterionRow[] = [
  {
    id: '1.2.4',
    name: 'Captions (live)',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'No live audio or video.',
  },
  {
    id: '1.2.5',
    name: 'Audio description (prerecorded)',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'No video.',
  },
  {
    id: '1.3.4',
    name: 'Orientation',
    level: 'AA',
    conformance: 'supports',
    remark: "Windows don't lock to an orientation.",
  },
  {
    id: '1.3.5',
    name: 'Identify input purpose',
    level: 'AA',
    conformance: 'supports',
    remark: 'Server sign-in fields and every email field declare their purpose, so autofill works.',
  },
  {
    id: '1.4.3',
    name: 'Contrast (minimum)',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      'An automated check on every change measures every text color against its background in light and dark mode, with all macOS accent colors and pane tints, at 4.5:1 (3:1 for large text). Marked file names measure at least 4.52:1. Two exceptions: with the macOS Increase contrast setting on, light mode can drop to about 4.18:1 on a brown pane tint, and the optional "App" date and size colors can put a pale accent color on text (about 2.4:1 with the gold accent).',
    issues: [440],
  },
  {
    id: '1.4.4',
    name: 'Resize text',
    level: 'AA',
    conformance: 'supports',
    remark:
      'App zoom (Command-plus and Command-minus, 75–150%) combines with the macOS text size setting, so text reaches 200% and beyond, and rows grow with it. App zoom alone stops at 150%.',
    issues: [441],
  },
  {
    id: '1.4.5',
    name: 'Images of text',
    level: 'AA',
    conformance: 'supports',
    remark: 'All text in the app is real text. There are no images of text.',
  },
  {
    id: '1.4.10',
    name: 'Reflow',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "Dialogs and Settings are one column wide, and their minimum widths grow with the text size. The main window has a minimum size of 950 by 550 points and a two-pane layout with columns. The file grid needs both dimensions, which WCAG allows, but the controls around it don't reflow. A manual check at large text sizes is pending.",
  },
  {
    id: '1.4.11',
    name: 'Non-text contrast',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      'Checkbox and switch borders reach about 3.7:1. Text field borders are 1.36:1 in light mode and 1.71:1 in dark mode. In light mode, the focus ring is under 3:1 with the yellow, gold, green, and orange accent colors, and the difference between the focused and the unfocused pane is about 1.09:1.',
    issues: [440],
  },
  {
    id: '1.4.12',
    name: 'Text spacing',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "File list rows have a fixed height. Larger line spacing fits the default row density but gets cut off in the Compact density, and shortened file names don't account for extra letter or word spacing. A manual text-spacing test is pending.",
  },
  {
    id: '1.4.13',
    name: 'Content on hover or focus',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "Tooltips appear on hover and on keyboard focus, stay until the pointer or focus leaves, and any key closes them. Moving the pointer onto a tooltip closes it, so it can't be hovered.",
    issues: [441],
  },
  {
    id: '2.4.5',
    name: 'Multiple ways',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'Applies to sets of web pages and sets of software. Cmdr is a single app.',
  },
  {
    id: '2.4.6',
    name: 'Headings and labels',
    level: 'AA',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark:
      'Settings uses headings for its sections, dialogs have a heading for their title, and both panes are named. Both panes\' file lists share the name "File list".',
    issues: [437],
    pending: ['V2'],
  },
  {
    id: '2.4.7',
    name: 'Focus visible',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      'Buttons, links, fields, and menus show a focus ring, and the file cursor has a colored background and outline. Which of the two panes has focus is very hard to see in light mode.',
    issues: [440],
  },
  {
    id: '2.4.11',
    name: 'Focus not obscured (minimum)',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      'Dialogs open in the middle of the window. Notifications appear at the top right and can cover the first rows of the right pane, including the cursor, until they close.',
    issues: [439],
  },
  {
    id: '2.5.7',
    name: 'Dragging movements',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "Resizing the panes, and copying or moving files by drag and drop, all have keyboard and menu alternatives. Reordering tabs and moving a tab to the other pane don't.",
    issues: [441],
  },
  {
    id: '2.5.8',
    name: 'Target size (minimum)',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      'Column headers, the function key bar, and the new-tab button are at least 24 by 24 pixels. The tab close button is 14 by 14 pixels, path segments above the file list are about 17 pixels tall, and the divider between the panes is 4 pixels wide.',
    issues: [441],
  },
  {
    id: '3.1.2',
    name: 'Language of parts',
    level: 'AA',
    conformance: 'supports',
    remark:
      'The language picker shows each language in its own name and marks which language that name is in, so a screen reader pronounces it right.',
  },
  {
    id: '3.2.3',
    name: 'Consistent navigation',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'Applies to sets of web pages and sets of software. The macOS menu bar is the same in every window.',
  },
  {
    id: '3.2.4',
    name: 'Consistent identification',
    level: 'AA',
    conformance: 'not-applicable',
    remark: 'Applies to sets of web pages and sets of software. Each command has one name everywhere in the app.',
  },
  {
    id: '3.3.3',
    name: 'Error suggestion',
    level: 'AA',
    conformance: 'supports',
    remark: 'Error messages say what happened and what to try next.',
  },
  {
    id: '3.3.4',
    name: 'Error prevention (legal, financial, data)',
    level: 'AA',
    conformance: 'supports',
    remark:
      'Deleting always asks first, and deleting permanently asks even when confirmations are turned off. Overwrites and name clashes open a dialog, moving to the trash can be undone, and copies and moves can be rolled back. (The Undo after moving to the trash is mouse-only today.)',
    issues: [439],
  },
  {
    id: '3.3.8',
    name: 'Accessible authentication (minimum)',
    level: 'AA',
    conformance: 'supports',
    remark:
      'License keys, API keys, and passwords can be pasted, and nothing asks the user to solve a puzzle or remember a code.',
  },
  {
    id: '4.1.3',
    name: 'Status messages',
    level: 'AA',
    conformance: 'partially-supports',
    remark:
      "Notifications, progress during file operations, Search result counts, and folder errors are announced. An empty folder, loading, the number of marked files, filter and command palette result counts, and the Settings search count aren't.",
    issues: [438, 439],
    pending: ['V4', 'V6', 'V9'],
  },
]

const section508Ch3: CriterionRow[] = [
  {
    id: '302.1',
    name: 'Without vision',
    conformance: 'partially-supports',
    remark:
      'Dialogs, Settings, menus, and forms are built for screen readers. The file list likely doesn\'t announce its cursor or marked files. If VoiceOver confirms that, this row becomes "Does not support".',
    issues: [437, 438],
    pending: allChecks,
  },
  {
    id: '302.2',
    name: 'With limited vision',
    conformance: 'partially-supports',
    remark:
      'Text grows past 200% with the macOS text size setting and app zoom, dark mode is supported, and text contrast is checked automatically. The focused pane and some focus rings have weak contrast with some accent colors.',
    issues: [440, 441],
  },
  {
    id: '302.3',
    name: 'Without perception of color',
    conformance: 'partially-supports',
    remark: 'Marked files and hidden files are told apart by color. Everything else uses symbols and text.',
    issues: [440],
  },
  { id: '302.4', name: 'Without hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  { id: '302.5', name: 'With limited hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  { id: '302.6', name: 'Without speech', conformance: 'supports', remark: 'Cmdr needs no speech input.' },
  {
    id: '302.7',
    name: 'With limited manipulation',
    conformance: 'partially-supports',
    remark:
      'Everything core works from the keyboard with remappable shortcuts, and no shortcut needs more than modifiers plus one key, so Sticky Keys works. Reordering tabs needs a drag, notification buttons need a pointer, and some click targets are smaller than 24 pixels.',
    issues: [439, 441],
  },
  {
    id: '302.8',
    name: 'With limited reach and strength',
    conformance: 'supports',
    remark: 'Cmdr is software and needs no physical reach.',
  },
  {
    id: '302.9',
    name: 'With limited language, cognitive, and learning abilities',
    conformance: 'partially-supports',
    remark:
      'Plain, conversational wording, confirmations before risky actions, undo and rollback, and the same menus everywhere. Notifications close before some users can act on them.',
    issues: [439],
  },
]

const section508Ch5: CriterionRow[] = [
  {
    id: '501.1',
    name: 'Scope, incorporation of WCAG 2.0 AA',
    conformance: 'see-other',
    seeLabel: 'See WCAG 2.2 tables',
    remark: 'The WCAG 2.2 tables cover every WCAG 2.0 criterion.',
  },
  {
    id: '502.2.1',
    name: 'User control of accessibility features',
    conformance: 'not-applicable',
    remark: "Cmdr isn't platform software.",
  },
  {
    id: '502.2.2',
    name: 'No disruption of accessibility features',
    conformance: 'supports',
    remark:
      "VoiceOver's keyboard commands pass through, macOS zoom works, and the menus are native. In the main window, the Tab key switches panes (a remappable shortcut), so macOS Full Keyboard Access doesn't move through the pane controls.",
    issues: [437],
  },
  {
    id: '502.3.1',
    name: 'Object information',
    conformance: 'partially-supports',
    remark:
      "Standard controls expose their name, role, and state to VoiceOver. File rows don't expose their kind (file or folder) or whether they're marked.",
    issues: [437],
    pending: ['V1', 'V3'],
  },
  {
    id: '502.3.2',
    name: 'Modification of object information',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'Checkboxes, switches, and dropdowns are standard controls that assistive technology can change.',
    pending: ['V7'],
  },
  {
    id: '502.3.3',
    name: 'Row, column, and headers',
    conformance: 'partially-supports',
    remark:
      "The queue window is a grid with headers. The Full view of the file list has column headers, but they aren't tied to the cells.",
    issues: [437],
  },
  {
    id: '502.3.4',
    name: 'Values',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'Progress bars, sliders, and number fields expose their values. Progress has no spoken text form yet.',
    issues: [438],
    pending: ['V4'],
  },
  {
    id: '502.3.5',
    name: 'Modification of values',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'Sliders, number fields, and dropdowns are standard controls that assistive technology can operate.',
    pending: ['V7'],
  },
  {
    id: '502.3.6',
    name: 'Label relationships',
    conformance: 'partially-supports',
    remark:
      'Fields are tied to their labels and descriptions throughout, except the license key error and two alert dialogs without a description.',
    issues: [438],
    pending: ['V5'],
  },
  {
    id: '502.3.7',
    name: 'Hierarchical relationships',
    conformance: 'partially-supports',
    remark:
      "Dialogs, menus, and the tab bar are structured. The Settings sidebar doesn't expose its subsections, and tabs aren't tied to their content.",
    issues: [437, 438],
    pending: ['V7'],
  },
  {
    id: '502.3.8',
    name: 'Text',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'All text in the app is real text that the macOS accessibility interface can read.',
    pending: ['V11'],
  },
  {
    id: '502.3.9',
    name: 'Modification of text',
    conformance: 'supports',
    remark: 'Text entry uses standard text fields.',
  },
  {
    id: '502.3.10',
    name: 'List of actions',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'Controls are standard buttons and widgets with standard actions.',
    pending: ['V8'],
  },
  {
    id: '502.3.11',
    name: 'Actions on objects',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'Assistive technology can press buttons and operate controls.',
    pending: ['V8'],
  },
  {
    id: '502.3.12',
    name: 'Focus cursor',
    conformance: 'partially-supports',
    remark:
      "Focus in dialogs and Settings is real keyboard focus. The cursor in the file list and in Search results isn't exposed to assistive technology.",
    issues: [437, 438],
    pending: ['V1', 'V6'],
  },
  {
    id: '502.3.13',
    name: 'Modification of focus cursor',
    conformance: 'partially-supports',
    remark:
      "Assistive technology can move focus between standard controls. Moving the VoiceOver cursor onto a file likely doesn't move Cmdr's cursor.",
    issues: [437],
    pending: ['V1'],
  },
  {
    id: '502.3.14',
    name: 'Event notification',
    conformance: 'partially-supports',
    remark: "See WCAG 4.1.3: some status changes are announced, others aren't.",
    issues: [438, 439],
    pending: ['V4'],
  },
  {
    id: '502.4',
    name: 'Platform accessibility features',
    conformance: 'not-applicable',
    remark: 'Applies to platforms. Cmdr is an app.',
  },
  {
    id: '503.2',
    name: 'User preferences',
    conformance: 'partially-supports',
    remark:
      "Cmdr follows the macOS system font, light and dark mode, accent color, text size, Reduce transparency, and Reduce motion. Increase contrast only changes pane tints, and Differentiate without color isn't used.",
    issues: [440],
  },
  {
    id: '503.3',
    name: 'Alternative user interfaces',
    conformance: 'not-applicable',
    remark: 'Cmdr provides no alternative user interface that works as assistive technology.',
  },
  { id: '503.4.1', name: 'Caption controls', conformance: 'not-applicable', remark: 'No video.' },
  { id: '503.4.2', name: 'Audio description controls', conformance: 'not-applicable', remark: 'No video.' },
  {
    id: '504.2',
    name: 'Content creation or editing',
    conformance: 'not-applicable',
    remark: "Cmdr isn't an authoring tool.",
  },
  {
    id: '504.2.1',
    name: 'Preservation of information provided for accessibility in format conversion',
    conformance: 'not-applicable',
    remark: "Cmdr isn't an authoring tool.",
  },
  { id: '504.2.2', name: 'PDF export', conformance: 'not-applicable', remark: "Cmdr isn't an authoring tool." },
  { id: '504.3', name: 'Prompts', conformance: 'not-applicable', remark: "Cmdr isn't an authoring tool." },
  { id: '504.4', name: 'Templates', conformance: 'not-applicable', remark: "Cmdr isn't an authoring tool." },
]

const section508Ch6: CriterionRow[] = [
  {
    id: '602.2',
    name: 'Accessibility and compatibility features',
    conformance: 'partially-supports',
    remark:
      'The Keyboard shortcuts window (in the Help menu) lists every keyboard command. This report lists the known limits, but no guide describes the other accessibility features yet.',
    issues: [443],
  },
  {
    id: '602.3',
    name: 'Electronic support documentation',
    conformance: 'supports',
    remark:
      "Every page of getcmdr.com is tested automatically for WCAG 2 AA issues in light and dark mode, and scored 100 on Lighthouse's accessibility audit.",
  },
  {
    id: '602.4',
    name: 'Alternate formats for non-electronic support documentation',
    conformance: 'not-applicable',
    remark: 'All documentation is electronic.',
  },
  {
    id: '603.2',
    name: 'Information on accessibility and compatibility features',
    conformance: 'partially-supports',
    remark: 'Support answers by email and through in-app feedback, but has no accessibility guide to point to yet.',
    issues: [443],
  },
  {
    id: '603.3',
    name: 'Accommodation of communication needs',
    conformance: 'supports',
    remark: 'All support is written, by email or in-app feedback. No phone call is needed.',
  },
]

const enClause4: CriterionRow[] = [
  {
    id: '4.2.1',
    name: 'Usage without vision',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.1.',
    issues: [437],
    pending: allChecks,
  },
  {
    id: '4.2.2',
    name: 'Usage with limited vision',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.2.',
    issues: [440],
  },
  {
    id: '4.2.3',
    name: 'Usage without perception of colour',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.3.',
    issues: [440],
  },
  { id: '4.2.4', name: 'Usage without hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  { id: '4.2.5', name: 'Usage with limited hearing', conformance: 'supports', remark: 'Cmdr uses no sound.' },
  {
    id: '4.2.6',
    name: 'Usage with no or limited vocal capability',
    conformance: 'supports',
    remark: 'Cmdr needs no speech input.',
  },
  {
    id: '4.2.7',
    name: 'Usage with limited manipulation or strength',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.7.',
    issues: [439, 441],
  },
  {
    id: '4.2.8',
    name: 'Usage with limited reach',
    conformance: 'supports',
    remark: 'Cmdr is software and needs no physical reach.',
  },
  { id: '4.2.9', name: 'Minimize photosensitive seizure triggers', conformance: 'supports', remark: 'See WCAG 2.3.1.' },
  {
    id: '4.2.10',
    name: 'Usage with limited cognition, language or learning',
    conformance: 'partially-supports',
    remark: 'See Section 508 302.9.',
    issues: [439],
  },
  {
    id: '4.2.11',
    name: 'Privacy',
    conformance: 'supports',
    remark:
      "Accessibility features don't expose private data. Passwords and keys use masked fields and are stored in the macOS Keychain.",
  },
]

const enClause5: CriterionRow[] = [
  {
    id: '5.1.2–5.1.7',
    name: 'Closed functionality',
    conformance: 'not-applicable',
    remark: 'Cmdr is open to assistive technology.',
  },
  {
    id: '5.2',
    name: 'Activation of accessibility features',
    conformance: 'supports',
    remark:
      'Zoom works from the keyboard, the View menu, and Settings. macOS accessibility settings apply with no extra steps.',
  },
  { id: '5.3', name: 'Biometrics', conformance: 'not-applicable', remark: 'Cmdr uses no biometric identification.' },
  {
    id: '5.4',
    name: 'Preservation of accessibility information during conversion',
    conformance: 'not-applicable',
    remark: 'Cmdr copies and archives files without converting their formats.',
  },
  { id: '5.5.1', name: 'Means of operation', conformance: 'not-applicable', remark: 'Applies to hardware.' },
  { id: '5.5.2', name: 'Operable parts discernibility', conformance: 'not-applicable', remark: 'Applies to hardware.' },
  {
    id: '5.6.1',
    name: 'Tactile or auditory status',
    conformance: 'not-applicable',
    remark: 'Cmdr has no locking or toggle keys of its own.',
  },
  {
    id: '5.6.2',
    name: 'Visual status',
    conformance: 'not-applicable',
    remark: 'Cmdr has no locking or toggle keys of its own.',
  },
  { id: '5.7', name: 'Key repeat', conformance: 'not-applicable', remark: 'Cmdr uses the macOS key repeat settings.' },
  {
    id: '5.8',
    name: 'Double-strike key acceptance',
    conformance: 'not-applicable',
    remark: 'Cmdr uses the macOS keyboard settings.',
  },
  {
    id: '5.9',
    name: 'Simultaneous user actions',
    conformance: 'supports',
    remark:
      'Shortcuts are modifiers plus one key, which Sticky Keys can enter one at a time. Nothing needs two pointer actions at once.',
  },
]

const enClause11: CriterionRow[] = [
  {
    id: '11.1–11.4',
    name: 'Software criteria that map to WCAG',
    conformance: 'see-other',
    seeLabel: 'See WCAG 2.2 tables',
    remark: 'Each of these criteria matches a WCAG 2.2 criterion of the same number, applied to software.',
  },
  {
    id: '11.5.1',
    name: 'Closed functionality',
    conformance: 'not-applicable',
    remark: 'Cmdr is open to assistive technology.',
  },
  {
    id: '11.5.2.1–11.5.2.3',
    name: 'Platform accessibility service support and use',
    conformance: 'see-other',
    seeLabel: 'See 11.5.2.5–11.5.2.17',
    remark:
      "Cmdr uses the macOS accessibility interface through WebKit, Apple's web engine. The rows below say how well each part works.",
  },
  {
    id: '11.5.2.4',
    name: 'Assistive technology',
    conformance: 'not-applicable',
    remark: "Cmdr isn't assistive technology.",
  },
  {
    id: '11.5.2.5',
    name: 'Object information',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.1.',
    issues: [437],
    pending: ['V1'],
  },
  {
    id: '11.5.2.6',
    name: 'Row, column, and headers',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.3.',
    issues: [437],
  },
  {
    id: '11.5.2.7',
    name: 'Values',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'See Section 508 502.3.4.',
    pending: ['V4'],
  },
  {
    id: '11.5.2.8',
    name: 'Label relationships',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.6.',
    issues: [438],
    pending: ['V5'],
  },
  {
    id: '11.5.2.9',
    name: 'Parent-child relationships',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.7.',
    issues: [437, 438],
    pending: ['V7'],
  },
  {
    id: '11.5.2.10',
    name: 'Text',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'See Section 508 502.3.8.',
    pending: ['V11'],
  },
  {
    id: '11.5.2.11',
    name: 'List of available actions',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'See Section 508 502.3.10.',
    pending: ['V8'],
  },
  {
    id: '11.5.2.12',
    name: 'Execution of available actions',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'See Section 508 502.3.11.',
    pending: ['V8'],
  },
  {
    id: '11.5.2.13',
    name: 'Tracking of focus and selection attributes',
    conformance: 'partially-supports',
    remark: "See Section 508 502.3.12. Marked files also aren't exposed as selected.",
    issues: [437],
    pending: ['V1', 'V3'],
  },
  {
    id: '11.5.2.14',
    name: 'Modification of focus and selection attributes',
    conformance: 'partially-supports',
    remark: 'See Section 508 502.3.13.',
    issues: [437],
    pending: ['V1'],
  },
  {
    id: '11.5.2.15',
    name: 'Change notification',
    conformance: 'partially-supports',
    remark: 'See WCAG 4.1.3.',
    issues: [438, 439],
    pending: ['V4', 'V9'],
  },
  {
    id: '11.5.2.16',
    name: 'Modifications of states and properties',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'See Section 508 502.3.2.',
    pending: ['V7'],
  },
  {
    id: '11.5.2.17',
    name: 'Modifications of values and text',
    conformance: 'not-evaluated',
    codeReview: 'supports',
    remark: 'See Section 508 502.3.5 and 502.3.9.',
    pending: ['V7'],
  },
  {
    id: '11.6.1',
    name: 'User control of accessibility features',
    conformance: 'not-applicable',
    remark: "Cmdr isn't platform software.",
  },
  {
    id: '11.6.2',
    name: 'No disruption of accessibility features',
    conformance: 'supports',
    remark: 'See Section 508 502.2.2.',
  },
  {
    id: '11.7',
    name: 'User preferences',
    conformance: 'partially-supports',
    remark: 'See Section 508 503.2.',
    issues: [440],
  },
  {
    id: '11.8.1–11.8.5',
    name: 'Authoring tools',
    conformance: 'not-applicable',
    remark: "Cmdr isn't an authoring tool.",
  },
]

const enClause12: CriterionRow[] = [
  {
    id: '12.1.1',
    name: 'Accessibility and compatibility features',
    conformance: 'partially-supports',
    remark: 'See Section 508 602.2.',
    issues: [443],
  },
  { id: '12.1.2', name: 'Accessible documentation', conformance: 'supports', remark: 'See Section 508 602.3.' },
  {
    id: '12.2.2',
    name: 'Information on accessibility and compatibility features',
    conformance: 'partially-supports',
    remark: 'See Section 508 603.2.',
    issues: [443],
  },
  { id: '12.2.3', name: 'Effective communication', conformance: 'supports', remark: 'See Section 508 603.3.' },
  { id: '12.2.4', name: 'Accessible documentation', conformance: 'supports', remark: 'See Section 508 602.3.' },
]

/** The report's tables, grouped by standard, in the VPAT's order. */
export const standards: StandardSection[] = [
  {
    id: 'wcag',
    title: 'WCAG 2.2 report',
    tables: [
      { id: 'wcag-a', title: 'Table 1: Success criteria, Level A', rows: wcagA },
      { id: 'wcag-aa', title: 'Table 2: Success criteria, Level AA', rows: wcagAA },
      {
        id: 'wcag-aaa',
        title: 'Table 3: Success criteria, Level AAA',
        note: 'Not evaluated. None of the standards in this report requires Level AAA.',
        rows: [],
      },
    ],
  },
  {
    id: 'section-508',
    title: 'Revised Section 508 report',
    tables: [
      { id: 's508-ch3', title: 'Chapter 3: Functional performance criteria', rows: section508Ch3 },
      { id: 's508-ch4', title: 'Chapter 4: Hardware', note: 'Not applicable. Cmdr is software.', rows: [] },
      { id: 's508-ch5', title: 'Chapter 5: Software', rows: section508Ch5 },
      { id: 's508-ch6', title: 'Chapter 6: Support documentation and services', rows: section508Ch6 },
    ],
  },
  {
    id: 'en-301-549',
    title: 'EN 301 549 report',
    tables: [
      { id: 'en-4', title: 'Clause 4: Functional performance statements', rows: enClause4 },
      { id: 'en-5', title: 'Clause 5: Generic requirements', rows: enClause5 },
      {
        id: 'en-6-8',
        title: 'Clauses 6–8: Two-way voice communication, ICT with video capabilities, and hardware',
        note: 'Not applicable. Cmdr has no voice or video communication and is software.',
        rows: [],
      },
      {
        id: 'en-9',
        title: 'Clause 9: Web',
        note: 'Not applicable. Cmdr is a desktop app, not web content. The getcmdr.com website is covered as documentation under clause 12.',
        rows: [],
      },
      {
        id: 'en-10',
        title: 'Clause 10: Non-web documents',
        note: 'Not applicable. Cmdr ships no non-web documents.',
        rows: [],
      },
      { id: 'en-11', title: 'Clause 11: Software', rows: enClause11 },
      { id: 'en-12', title: 'Clause 12: Documentation and support services', rows: enClause12 },
      {
        id: 'en-13',
        title: 'Clause 13: ICT providing relay or emergency service access',
        note: 'Not applicable. Cmdr provides no relay or emergency services.',
        rows: [],
      },
    ],
  },
]

/** The open issues behind the report's gaps, most important first. */
export const gapIssues: { number: number; title: string }[] = [
  { number: 437, title: "The file list doesn't yet speak to VoiceOver: cursor, marked files, position, and file kind" },
  {
    number: 438,
    title: "Search and Settings cursors, empty folders, selection counts, and form errors aren't announced",
  },
  { number: 439, title: "Notifications can't be reached or kept from the keyboard, and Undo trash lives only in one" },
  { number: 440, title: 'The focused pane, marked files, and some focus rings rely on faint color' },
  {
    number: 441,
    title:
      "Tab reordering needs a drag, some click targets are small, tooltips can't be hovered, viewer keys can't be remapped, and app zoom stops at 150%",
  },
  { number: 443, title: "No guide to Cmdr's accessibility features yet" },
  { number: 267, title: 'In-app menus (volume chooser, favorites) under VoiceOver' },
]

/** The screen-reader checks still to run. Each one's rows are the rows whose `pending` names it. */
export const voiceOverChecks: { id: VoiceOverCheck; what: string }[] = [
  {
    id: 'V1',
    what: 'Browse a folder: VoiceOver reads each file name as the cursor moves, says folder or file, and the position in the list.',
  },
  { id: 'V2', what: "Switch panes: it says which pane and folder you're in." },
  { id: 'V3', what: 'Mark files: it says a marked file is selected, and how many are.' },
  { id: 'V4', what: "Copy files: the progress dialog's title, progress value, and completion message." },
  { id: 'V5', what: "Rename: the field's name, and an announced error for an invalid name." },
  { id: 'V6', what: 'Search: the result count, and the highlighted result while arrowing.' },
  { id: 'V7', what: "Settings: section names, a control's label and value, and the new value after a change." },
  { id: 'V8', what: 'Delete confirmation: title, message, and buttons, and pressing a button works.' },
  { id: 'V9', what: 'Notifications: one is read out without moving focus.' },
  { id: 'V10', what: 'In-app menus (volume chooser, favorites): rows are read while arrowing.' },
  { id: 'V11', what: 'Read text: an error message and a text file in the viewer are read in full.' },
  { id: 'V12', what: "Reading order: VoiceOver's order in a dialog and in Settings matches the screen." },
]

/** Number of rows still waiting on any screen-reader check. */
export const pendingRowCount = standards
  .flatMap((standard) => standard.tables.flatMap((table) => table.rows))
  .filter((row) => row.pending?.length).length
