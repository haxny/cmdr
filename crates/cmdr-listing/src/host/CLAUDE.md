# The host seams

Everything the listing engine needs from the application around it, as named traits instead of reaches upward. Cmdr
answers them in `apps/desktop/src-tauri/src/listing_host/`.

## Must-knows

- **Add a seam here, ❌ never a dependency on the app.** `index-crate-isolation` keeps `tauri`, `tauri-specta`, and
  `cmdr` out of this crate's tree. A question that's pure string work on a `&str` isn't a seam: it goes to `cmdr_fs`.
- **Every seam degrades to a no-op and has a `testing` fake.** The `No*` defaults are public, so a host can pass one;
  `Listings::detached()` uses them all.
- **A seam is called per read or per mutation, ❌ never per entry**: every one is a `dyn` call. Hoist and batch
  (`ListingIndex::enrich` takes a slice).
- **No seam, observer, or overlay runs under an engine lock.** Resolve its answer first and pass the value in.
- **The engine reports facts; the host owns the words.** `ListingEvents` carries typed facts; wire payloads, analytics,
  and error classification are the host's.
- **`ListingIndex::enrich` writes only the seven `recursive_*` fields, and only on non-symlink folders.** The off-lock
  index-size refresh clones exactly those rows; a fake must honor it.
- **Overlay rows reach a pane and nothing else**, ❌ never a `Volume`: `DETAILS.md` § "Overlays".
- **Observers hear a close after the cache removal**, and their arming belongs to a backend, ❌ never a frontend
  subscription: `DETAILS.md` § "Observers".

## Module map

- `mod.rs`: the public re-exports (the seam surface) and `ScratchVisibility`, engine state the host sets.
- `events.rs`: `ListingEvents`, `ListingEnd`, `StalledOn`, `NoEvents`.
- `volumes.rs`: `ListingVolumes`, `Unregistered`, `NoVolumes`.
- `index.rs`: `ListingIndex`, `NoIndex`.
- `overlays.rs`: `ListingOverlay`, a list in registration order.
- `observers.rs`: `ListingObserver`, `OpenedListing`, a list in registration order.
- `leases.rs`: `ForegroundLeases`, `ForegroundHold`, `NoLeases`.
- `locale.rs`: `ReadingLocale`, `NoLocale`. `mounts.rs`: `MountTable`, `MountFacts`, `NoMountTable`.

What each seam replaces, its no-op, and the reasoning behind overlays and observers: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
