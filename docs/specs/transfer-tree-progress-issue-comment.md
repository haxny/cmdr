Hey Jiri

Thanks a lot for this one, and for #395 too! Your three pain points were spot on, and they shaped the whole design:

1. **Which folder is still untouched?** (80% of the bytes done, a whole folder still waiting)
2. **Which `index.html` is this?** (a bare file name means nothing when every folder has one)
3. **Did folders 1–3 land?** (when something goes wrong in folder 4, without reading logs)

It's built now. Here's what it does.

## What you get

One shared **live transfer tree**, in the copy dialog AND inline in the queue window. Copy, move, and compress all have
it.

- **The rows are your selection, as a tree.** Paths are relative to the folder you started from. Folder rows have their
  own small bar plus "312 / 1,000 files · 1.2 GB / 4.0 GB". A file in flight has a tiny per-file bar. Rows show waiting,
  copying, done (checkmark), skipped, or "couldn't finish" (with the reason from the typed error pipeline, as you
  suggested). That includes the destination side: a full disk marks the file it was writing.
- **Compact folders**: a chain like `a/b/c` with nothing else in it is one row, like in VS Code or GitHub.
- **The tree follows the work.** The folder being copied opens, and it closes again once the work moves on. Open or
  close a folder yourself, and it stays the way you left it. Scroll away, and following pauses with a "Follow" button
  (or F) to jump back, like a log viewer.
- **Sticky folder headers** (VS Code's sticky scroll): when you scroll deep into a big folder, its parents stay pinned
  at the top, with live counts. This one wasn't in the plan, but a deep tree was confusing without it.
- **Everything stays visible**, finished rows included (with a checkmark). 5,000 top-level files is a 5,000-row list,
  virtualized.
- **Folder bars follow the bottleneck.** The backend computes the ETA as the max of the bytes ETA and the files ETA, so
  the folder bars fill along whichever axis actually decides when you're done (175k tiny files and 5 GB tell different
  stories).
- **Parallel SMB copies** show several active rows at once, in whatever folders they are.
- **The dialog is resizable** (up to ~8 rows before you drag it), and the header has an "N left" next to the speed and
  ETA. A conflict prompt shrinks it back to fit, and the tree comes back afterward exactly as you left it.
- **Queue**: the tree sits right under each running row, in the same card, always visible. If several operations run at
  once (they can, on different drives), each has its own tree, collapsible.
- **Keyboard and screen readers**: a proper ARIA tree, arrow keys to move, Left/Right to collapse/expand. Each row's
  screen reader name starts with what's on screen, so voice control finds it too.

## What we took from your proposal

- **Per-item totals from the scan.** We went one level further: totals for EVERY folder, at any depth. So the scan now
  records the tree (for network drives it kept top-level totals only).
- **Typed reasons for failed and skipped rows**, never new prose.
- **A snapshot for a window that opens mid-job.** It's the same call the view uses for every update: each view asks for
  just the rows it shows, on the existing progress tick (at most 5 times a second). So the IPC stays bounded by the
  viewport, even for a million files (the tree itself is ~50 bytes per file in memory).
- **Per-source completion for every driver.** The SMB/MTP copy drivers didn't emit `write-source-item-done` at all, so
  network copies never deselected gradually. Fixed along the way, and a pane now only listens to its own operation, so a
  copy on another drive can't deselect a same-named file.

## Where we went further, and why

- **A tree.** "Which folder" is often a folder inside the folder you picked, and a flat list of items can't answer that.
- **Always visible.** The current work is the thing you want to see without clicking (same reasoning as on #395), so
  there's no collapsed "Items" section.
- **No "1,204 done" grouping.** Finished rows are exactly the "did it land?" answer, so they stay.
- **One processing order everywhere.** This one surprised me: local copies went through ONE flat list sorted by file
  name across all folders, so `b/aaa.txt` copied before `a/zzz.txt` and every folder finished near the end. Network
  copies walked in whatever order the server listed. Now every driver starts files in the order the tree shows (folders
  first, then files, by name), so the rows below the active one really are what's coming next. (The pane's sort column
  doesn't affect copy order anymore.)
- **The tree can never break a copy.** It only watches. If it hits a bug, it turns itself off and the copy carries on
  exactly as before.

## Not in this round

- "Skip the rest of this folder". Good idea, but it needs a per-folder cancel in both copy engines, so it gets its own
  issue.
- Keeping the tree after the operation ends, for the finished summary (#261) and a failed queue row's details.
- Trees for delete and trash, and for edits inside an existing zip.
- Showing the active rows to MCP agents, and remembering the dialog's resized height.

If you want to poke at it without a real copy, Debug → Components → Transfer tree plays a few scripted shapes (parallel
SMB, failures, a deep tree). Thanks again, this turned out to be a really nice one! :)
