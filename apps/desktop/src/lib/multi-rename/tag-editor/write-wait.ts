/**
 * How long a write press waits for the calls still out (an edit a blur just sent, a slow read) before it answers.
 * The panel shows "Finishing the last change…" after `WAITING_SHOWN_AFTER_MS`, so a quick wait doesn't flash it, and
 * gives up after `WRITE_WAIT_LIMIT_MS` with a worded reason, so a hung call can't swallow the press.
 */

export const WAITING_SHOWN_AFTER_MS = 250

export const WRITE_WAIT_LIMIT_MS = 30_000
