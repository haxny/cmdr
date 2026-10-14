/**
 * Cursor and pane-focus helpers for the Cmdr Playwright E2E tests.
 *
 * Move the cursor to a named file (`moveCursorToFile`), focus a pane
 * (`focusPane`), skip the ".." parent entry (`skipParentEntry`), and re-anchor
 * keyboard focus on the explorer after MCP-driven actions (`ensureExplorerFocused`).
 */

import { waitBudget } from '../wait-budget.js'
import { expect } from '@playwright/test'
import { ensureMcpClient, mcpCall } from '../../e2e-shared/mcp-client.js'
import { type PageLike, findFileIndex, pollUntil } from './core.js'

// ── Cursor helpers ──────────────────────────────────────────────────────────

/**
 * If the cursor is on the ".." parent entry, moves it onto the first real entry.
 *
 * Uses the MCP `move_cursor` path (via `moveCursorToFile`), NOT an OS-keyboard
 * ArrowDown. Under Linux Xvfb the OS keystroke can vanish (unreliable X11 focus
 * delivery), leaving the cursor stuck on ".." past the deadline — the flaky
 * `skipParentEntry: cursor did not leave ".."` on the Docker lane, which took
 * down the F5/F6/F8/Space setup in `app.spec.ts`. MCP round-trips through the
 * backend with no keystroke, so it's deterministic. Moving to the first non-".."
 * entry equals one ArrowDown from ".." (which sorts first in the listing).
 */
export async function skipParentEntry(tauriPage: PageLike): Promise<void> {
  const cursorText = await tauriPage.evaluate<string>(`(function() {
        var entry = document.querySelector('.file-pane.is-focused .file-entry.is-under-cursor')
          || document.querySelector('.file-entry.is-under-cursor');
        if (!entry) return '';
        return entry.getAttribute('data-filename') || '';
    })()`)
  if (cursorText !== '..') return

  const firstReal = await tauriPage.evaluate<string>(`(function() {
        var pane = document.querySelector('.file-pane.is-focused') || document.querySelector('.file-pane');
        if (!pane) return '';
        var entries = pane.querySelectorAll('.file-entry');
        for (var i = 0; i < entries.length; i++) {
            var name = entries[i].getAttribute('data-filename') || '';
            if (name && name !== '..') return name;
        }
        return '';
    })()`)
  if (!firstReal) throw new Error('skipParentEntry: focused pane has no non-".." entry to move to')
  const moved = await moveCursorToFile(tauriPage, firstReal)
  if (!moved) throw new Error('skipParentEntry: could not move the cursor off ".." onto ' + firstReal)
}

/**
 * Restores focus to `.dual-pane-explorer` (the keydown-handler host) before
 * an OS-keyboard test press. After MCP-driven actions like `move_cursor` the
 * focused element can drift to `<body>`, in which case `tauriPage.keyboard.press`
 * never reaches the explorer's handler. Mirrors the focus-recovery loop in
 * `ensureAppReady`, scoped to a single explicit call from a test mid-flow.
 *
 * Throws via `expect.poll` if focus can't be returned to the explorer in 2s
 * (an unexpected modal overlay or a missing explorer would do it).
 */
export async function ensureExplorerFocused(tauriPage: PageLike): Promise<void> {
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
          var ae = document.activeElement;
          if (!ae || !ae.closest || ae.closest('.dual-pane-explorer') === null) {
            var explorer = document.querySelector('.dual-pane-explorer');
            if (explorer) explorer.focus();
            ae = document.activeElement;
          }
          return !!(ae && ae.closest && ae.closest('.dual-pane-explorer') !== null);
        })()`),
      { timeout: waitBudget(2000) },
    )
    .toBeTruthy()
}

/**
 * Focuses the requested pane (0 = left, 1 = right) so subsequent keyboard
 * input reaches the pane's handler. After MCP-driven actions like `move_cursor`,
 * `mcp-volume-select`, or `mcp-nav-to-path`, the DOM `.file-pane.is-focused`
 * marker can be absent (no pane is the "active" one), which means
 * `DualPaneExplorer.handleKeyDown`'s `getPaneRef(focusedPane)` returns undefined
 * and the keydown handler no-ops. We click the pane directly — the same gesture
 * a real user would use — to set focus deterministically, then poll the
 * `.is-focused` marker to confirm the click landed.
 *
 * Replaces the "toggle twice" idiom that depended on a specific starting
 * focus state (two `switch_pane` toggles only return to the originally-focused
 * pane; if no pane is focused, two toggles do nothing).
 */
export async function focusPane(tauriPage: PageLike, paneIndex: 0 | 1): Promise<void> {
  // Click the pane to set focus. Click coordinates come from the pane's bounding
  // rect (some inset to avoid hitting edge handles). The click is dispatched as
  // a synthetic MouseEvent so we don't depend on the OS pointer position.
  await tauriPage.evaluate(`(function(){
    var pane = document.querySelectorAll('.file-pane')[${String(paneIndex)}];
    if (!pane) throw new Error('pane ' + ${String(paneIndex)} + ' not found');
    var rect = pane.getBoundingClientRect();
    var x = rect.left + Math.min(40, rect.width / 2);
    var y = rect.top + Math.min(40, rect.height / 2);
    var opts = { bubbles: true, cancelable: true, view: window, clientX: x, clientY: y, button: 0 };
    pane.dispatchEvent(new MouseEvent('mousedown', opts));
    pane.dispatchEvent(new MouseEvent('mouseup', opts));
    pane.dispatchEvent(new MouseEvent('click', opts));
  })()`)
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(
          `document.querySelectorAll('.file-pane')[${String(paneIndex)}]?.classList.contains('is-focused') === true`,
        ),
      { timeout: waitBudget(3000) },
    )
    .toBeTruthy()
}

/** Each pane's cursor row and whether a row follows it, read in one payload. */
interface PaneCursors {
  /** `data-filename` of the cursor row per pane, `''` where a pane shows none. */
  names: [string, string]
  /** Whether a rendered row follows the cursor, per pane. */
  hasNext: [boolean, boolean]
}

async function readPaneCursors(tauriPage: PageLike): Promise<PaneCursors> {
  return tauriPage.evaluate<PaneCursors>(`(function() {
    var names = [], hasNext = [];
    var panes = document.querySelectorAll('.file-pane');
    for (var i = 0; i < 2; i++) {
      var rows = panes[i] ? Array.prototype.slice.call(panes[i].querySelectorAll('.file-entry')) : [];
      var at = rows.findIndex(function(r) { return r.classList.contains('is-under-cursor'); });
      names.push(at >= 0 ? (rows[at].getAttribute('data-filename') || '') : '');
      hasNext.push(at >= 0 && at < rows.length - 1);
    }
    return { names: names, hasNext: hasNext };
  })()`)
}

/** `document.activeElement` as `<tag.class>`, for saying where a key landed. */
async function describeActiveElement(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(function() {
    var el = document.activeElement;
    if (!el) return 'nothing';
    if (el === document.body) return '<body>';
    var cls = typeof el.className === 'string' && el.className ? '.' + el.className.trim().split(/\\s+/).join('.') : '';
    return '<' + el.tagName.toLowerCase() + cls + '>';
  })()`)
}

/**
 * Presses ↓ the way a keypress lands (dispatched on `document.activeElement`) and asserts
 * that it moved the cursor in `pane` and ONLY there. That's the whole keyboard-focus
 * contract in one probe: a key that lands on `<body>` after a focus loss moves nothing, and
 * a focus model that drifted to the other pane moves the wrong cursor.
 *
 * ❗ Asserts on `.is-under-cursor` and `data-filename`, ❌ never `aria-selected` or which
 * element holds DOM focus: both are free to change while the keyboard behavior is frozen.
 */
export async function expectArrowDownMovesCursorIn(tauriPage: PageLike, pane: 'left' | 'right'): Promise<void> {
  const index = pane === 'left' ? 0 : 1
  const other = 1 - index
  const before = await readPaneCursors(tauriPage)
  expect(before.names[index], `the ${pane} pane shows no cursor row before ↓`).not.toBe('')
  expect(before.hasNext[index], `the ${pane} pane's cursor (${before.names[index]}) is on its last row`).toBe(true)

  const pressedOn = await describeActiveElement(tauriPage)
  await tauriPage.keyboard.press('ArrowDown')

  await expect
    .poll(async () => (await readPaneCursors(tauriPage)).names[index], {
      timeout: waitBudget(3000),
      message: `↓ (dispatched on ${pressedOn}) should move the ${pane} pane's cursor off ${before.names[index]}`,
    })
    .not.toBe(before.names[index])
  const after = await readPaneCursors(tauriPage)
  expect(after.names[other], `↓ must not move the other pane's cursor`).toBe(before.names[other])
}

/**
 * Moves the cursor to a specific file by name in the focused pane.
 *
 * Uses the MCP `move_cursor` tool, which jumps directly to the target file
 * instead of pressing ArrowDown N times. Falls back to `false` if the file
 * isn't in the focused pane's listing (matching the prior behavior).
 *
 * The focused-pane detection reads `.file-pane.is-focused` from the DOM, so
 * the signature stays compatible with the old keyboard-based version. Tests
 * that explicitly exercise arrow-key cursor movement (`app.spec.ts`) keep
 * their own keyboard-driven helper and don't use this function.
 */
export async function moveCursorToFile(tauriPage: PageLike, targetName: string): Promise<boolean> {
  // Wait for the target to appear in the focused pane's listing instead of
  // reading once. A file-op spec's `recreateFixtures` (beforeEach) deletes then
  // recreates `left/` on disk; the file watcher's debounced remove/create diffs
  // can drain just AFTER `ensureAppReady`'s files-present poll, briefly emptying
  // the pane (observed: left pane focused, `pane0Count` 0). A one-shot read
  // caught that reload window and returned false. Polling waits out the transient
  // empty; a file that's genuinely absent still fails after the deadline (returns
  // false, no masking). 5 s is failure headroom under parallel-shard load, well
  // within the 15 s per-test budget.
  const listed = await pollUntil(
    tauriPage,
    async () => {
      const probe = await findFileIndex(tauriPage, targetName)
      return !('error' in probe) && probe.targetIndex >= 0
    },
    waitBudget(5000),
  )
  if (!listed) return false

  // Determine which pane is focused so we can target the right one via MCP.
  const focusedPane = await tauriPage.evaluate<'left' | 'right' | null>(`(function() {
        var panes = document.querySelectorAll('.file-pane');
        for (var i = 0; i < panes.length; i++) {
            if (panes[i].classList.contains('is-focused')) {
                return i === 0 ? 'left' : 'right';
            }
        }
        return null;
    })()`)
  const pane: 'left' | 'right' = focusedPane ?? 'left'

  await ensureMcpClient(tauriPage)
  await mcpCall('move_cursor', { pane, filename: targetName })

  // Confirm the cursor landed on the target file. `move_cursor` round-trips AND
  // flushes the backend PaneStateStore before replying (DualPaneExplorer.moveCursor
  // → syncStateToMcpNow), so the backend cursor is fresh by the time this returns —
  // a follow-up copy/move/delete won't read a stale cursor-on-`..`. This poll only
  // covers the DOM render tick on a green run — it resolves the moment the cursor
  // lands and never reaches the budget. 8 s is failure headroom for the shared
  // Docker VM under load, where the render tick stretches.
  // Safe to keep at 8 s: this is the only bumped budget inside moveCursorToFile,
  // and the helper isn't called from ensureAppReady, so it never stacks.
  return pollUntil(
    tauriPage,
    async () => {
      return tauriPage.evaluate<boolean>(`(function() {
                var pane = document.querySelector('.file-pane.is-focused');
                if (!pane) return false;
                var entry = pane.querySelector('.file-entry.is-under-cursor');
                if (!entry) return false;
                return entry.getAttribute('data-filename') === ${JSON.stringify(targetName)};
            })()`)
    },
    waitBudget(8000),
  )
}
