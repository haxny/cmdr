# Listing host

This app as the listing engine's host: the mirror of `crates/cmdr-listing/src/host/`. `install` builds the app's one
engine at the top of `setup()`; `listings()` hands it out.

## Must-knows

- **Every directory change enters through the composer, `directory_changed`**: index, then cloud badges, then panes.
  ❌ Never call `Listings::directory_changed` directly from app code: the index and the badges would miss the change.
- **An adapter resolves its subsystem per call, ❌ never at construction.** `install` runs before `index_host::install`,
  so capturing `index()` there would win its `OnceLock` with an unconfigured index.
- **A read before `install` is loud** (`log_error!`, a separate detached engine), so the slot stays free. Only the test
  binary builds its engine on first read (`test_listings`).
- **Observer order is a contract**: the change watch, git arming, index sizes. § "The observers".
- **Wire payloads and every word the user sees live in `events.rs`**, names and derives fixed so `bindings.ts` doesn't
  move. The engine reports facts; this module owns the policy on them.
- **App-side tests share ONE engine**, so they keep unique ids (`TestListing` derives one); a cell that reads reports
  builds its own with `engine_reporting_to`.

## Module map

- `mod.rs`: `install`, `listings()`, the composer, the test engines. `backends.rs`: `AppListings`, the backends'
  `cmdr_fs` `ListingHost`. `events.rs`: `TauriListingEvents` and the wire structs.
- One adapter per seam: `volumes.rs`, `index.rs`, `leases.rs`, `locale.rs`, `mounts.rs`. `tests/`: the app-side cells.

The adapters, the composer's order, the observer order, and the tests: `DETAILS.md`. Read it before any non-trivial work
here: editing, planning, reorganizing, or advising.
