# Volume backends

What a caller reaching a backend from the app still has to know. Trait shape, capabilities, streaming patterns,
"Building a new volume": `../CLAUDE.md` + `../DETAILS.md`.

## Module map

- **Every backend is a crate**, imported by crate name at its call sites, never re-exported through here:
  `cmdr-local` (`LocalPosixVolume`, the local disk: `crates/cmdr-local/CLAUDE.md`), `cmdr-archive`, `cmdr-smb`,
  `cmdr-sftp`, `cmdr-webdav`, `cmdr-adb`, `cmdr-mtp`, and `cmdr-git` (the read-only `GitPortalVolume` a
  `.git/<category>/` path routes to). `InMemoryVolume` rides with the trait in `cmdr-fs`. ❌ Don't add a
  `pub use <crate>::*;` module here to spare a call site the crate name: it puts the backend back in a directory it
  doesn't live in, and it becomes the place app-side tests drift to.
- **A crate backend's app-side tests live beside the app code they assert on**, not here. The local backend's are in
  `listing_host/tests/` (they assert on the app's `AppListings`), with the SMB pane-close watcher and archive watch
  cells; SMB's transfer and remote-archive suites and the shared `smb_test_support.rs` are in
  `operations_host/backend_suites/`, and its index scan and media fetch cells in `file_system/volume/`. Which side a
  cell belongs on: `crates/cmdr-smb/DETAILS.md` § "Which side a test lives on".
- The app-side tracker, provider, and connect wiring for ADB and MTP live in `src-tauri/src/adb/` and
  `src-tauri/src/mtp/`, not here.

## Must-knows a caller needs

The local backend's own must-knows (the one no-replace rename, durable landings, atomic force-rename, hidden-file
folding) are `crates/cmdr-local/CLAUDE.md`. MTP is `crates/cmdr-mtp`; the three below are what a caller reaching a
backend from the app still has to know.

- **Feed the progress callbacks** in `list_directory` and in a copy SCAN (`scan_for_copy_batch_with_boundary`);
  ❌ never quiet one to `_on_progress`. They drive the pane's only "Loaded N files…" readout and the transfer dialog's
  only climbing counter, and the scan one is the watchdog's proof the device is answering: a silent backend gets cut
  off as unresponsive.
- **MTP has no single-file stat**, so `get_metadata` lists the whole parent: avoid it in hot paths. Ranged reads and
  read sessions are canonical in `crates/cmdr-mtp/src/connection/CLAUDE.md`.
- **`cmdr_mtp::volume::testing` is the ONLY way a test outside the backend reads it**: two numbers out
  (`list_directory_call_count`, `reset_list_directory_call_count`) and one in (`set_read_window`), gated on
  `any(test, feature = "testing")`, ❌ never `cfg(test)` alone. Don't grow it into a way to reach backend state.

The SMB auto-upgrade lifecycle, supersede-vs-unmount, and the per-backend decisions that stay app-side: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
