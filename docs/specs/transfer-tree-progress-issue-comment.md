Hey Jiri

Thanks a lot for this one, and for #395 too! Your three pain points were spot on, and they shaped the whole design:

1. **Which folder is still untouched?** (80% of the bytes done, a whole folder still waiting)
2. **Which `index.html` is this?** (a bare file name means nothing when every folder has one)
3. **Did folders 1–3 land?** (when something goes wrong in folder 4, without reading logs)

So I thought about it properly, and we're building it now. Here's what it'll look like.

## The design

One shared **live transfer tree**, used in the copy dialog AND inline in the queue window. Copy, move, and compress all
get it.

- **The rows are your selection, as a tree.** Paths are relative to the folder you started from. Folder rows get their
  own small bar plus "312 / 1,000 files · 1.2 / 4.0 GB". A file in flight gets a tiny per-file bar. Rows show waiting,
  copying, done (checkmark), skipped, or failed (with the reason from the typed error pipeline, as you suggested).
- **Compact folders**: a chain like `a/b/c` with nothing else in it is one row, like in VS Code or GitHub.
- **The tree follows the work.** The folder being copied opens, and it closes again once it's done. If you open or close
  a folder yourself, it stays the way you left it. Scroll away, and following pauses with a "Follow" button to jump back
  (like a log viewer).
- **Everything stays visible**, finished rows included (with a checkmark). 5,000 top-level files is a 5,000-row list,
  virtualized.
- **Folder bars follow the bottleneck.** The backend already computes the ETA as the max of the bytes ETA and the files
  ETA, so the folder bars fill along whichever axis actually decides when you're done (175k tiny files and 5 GB tell
  different stories).
- **Parallel SMB copies** show several active rows at once, in whatever folders they are.
- **The dialog becomes resizable** (~8 rows by default), and the header gets an "N left" next to the speed and ETA.
- **Queue**: the tree sits right under each running row, in the same card, always visible. If several operations run at
  once (they can, on different drives), each gets its own tree, collapsible.
- **Keyboard and screen readers**: a proper ARIA tree, arrow keys to move, Left/Right to collapse/expand.

## What we took from your proposal

- **Per-item totals from the scan.** We go one level further: totals for EVERY folder, at any depth. So the scan now
  records the tree (for network drives it kept top-level totals only).
- **Typed reasons for failed and skipped rows**, never new prose.
- **A snapshot for a window that opens mid-job.** It's the same call the view uses for every update: each view asks for
  just the rows it shows, triggered by the existing progress tick. So the IPC stays bounded by the viewport, even for a
  million files.
- **Per-source completion for every driver.** The SMB/MTP copy drivers didn't emit `write-source-item-done` at all, so
  network copies never deselected gradually. That gets fixed along the way.

## Where we went further, and why

- **A tree.** "Which folder" is often a folder inside the folder you picked, and a flat list of items can't answer that.
- **Always visible.** The current work is the thing you want to see without clicking (same reasoning as on #395), so
  there's no collapsed "Items" section.
- **No "1,204 done" grouping.** Finished rows are exactly the "did it land?" answer, so they stay.
- **One processing order everywhere.** This one surprised me: local copies went through ONE flat list sorted by file
  name across all folders, so `b/aaa.txt` copied before `a/zzz.txt` and every folder finished near the end. Network
  copies walked in whatever order the server listed. Now every driver starts files in the order the tree shows (folders
  first, then files, by name), so the rows below the active one really are what's coming next.

## Deferred

- "Skip the rest of this folder". Good idea, but it needs a per-folder cancel in both copy engines, so it gets its own
  issue.
- The finished summary reusing the tree (#261).
- Delete/trash trees, and edits inside an existing zip.

The full plan is in `docs/specs/transfer-tree-progress.md` if you're curious. Thanks again, this is going to be a really
nice one! :)
