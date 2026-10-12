//! Orphan-listing backstop reaper tests.
//!
//! These pin the defense-in-depth reaper that catches listings whose explicit
//! `list_directory_end` IPC was never delivered. The crux is that the reaper keys on
//! `last_accessed_ms` (refreshed on every live-pane access), NOT `created_at` (stamped
//! once), so a long-open-but-still-used listing is never evicted. The positive test
//! proves a stale listing ends the way a pane's close does, observers told after
//! the cache removal; the negative test proves a freshly-touched listing survives
//! even when it was created long ago. That the app's change watch goes with an
//! ended listing is `listing_host/tests/caching_reaper_test.rs`.
//!
//! Every sweep here is scoped to the ids the test names, the way the reaper's
//! own scoped entry point is.

use std::sync::Arc;

use super::operations::ListingLookupError;
use super::orphan_reaper::{ORPHAN_IDLE_WINDOW, orphan_ids};
use super::testing::{Heard, RecordingObserver, TestEngine, TestListing};

/// An engine whose observer records every open and close.
fn observed_engine() -> (TestEngine, Arc<RecordingObserver>) {
    let observer = RecordingObserver::new();
    let engine = TestEngine::builder().observer(observer.clone()).build();
    observer.attach(&engine);
    (engine, observer)
}

// ---- pure helper: orphan_ids ------------------------------------------------

#[test]
fn orphan_ids_flags_only_listings_idle_past_the_window() {
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let now = 100 * window_ms; // far enough that subtractions don't underflow

    let stamps = [
        ("fresh", now),                      // idle 0
        ("recent", now - 1),                 // idle 1 ms
        ("just_under", now - window_ms + 1), // idle window-1: NOT orphan
        ("exactly", now - window_ms),        // idle == window: orphan (>=)
        ("ancient", now - 10 * window_ms),   // idle 10x window: orphan
    ];

    let mut ids = orphan_ids(now, window_ms, stamps.iter().map(|(id, ms)| (*id, *ms)));
    ids.sort();

    assert_eq!(ids, vec!["ancient".to_string(), "exactly".to_string()]);
}

#[test]
fn orphan_ids_empty_for_all_fresh() {
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let now = 1_000_000_000u64;
    let stamps = [("a", now), ("b", now - 5), ("c", now - 100)];
    assert!(orphan_ids(now, window_ms, stamps.iter().map(|(id, ms)| (*id, *ms))).is_empty());
}

// ---- reap: positive (orphan + its watcher torn down together) ----------------
//
// We inject `now_ms` and `window_ms` because the real idle clock is relative to
// process start, so a genuine 6 h gap can't be produced in a unit test. The listing
// is stamped `last_accessed_ms = 0`; calling the reaper with `now = window` makes its
// idle time exactly the window → orphan.

#[test]
fn reaper_evicts_stale_listing_and_its_watcher_together() {
    let (engine, observer) = observed_engine();
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

    // Last access at the epoch (ancient).
    let listing = TestListing::new()
        .path("/orphan")
        .last_accessed_ms(0)
        .insert(&engine, "orphan");
    assert!(listing.is_cached(), "precondition: listing is cached");

    // now = window → idle == window → orphan.
    let reaped = engine
        .inner
        .reap_orphaned_listings_at_for(window_ms, window_ms, &[listing.id()]);

    assert!(
        reaped.contains(&listing.id().to_string()),
        "reaper should report the orphaned listing"
    );
    assert!(!listing.is_cached(), "reaper must remove the cache entry");
    assert_eq!(
        observer.heard(),
        [Heard::Closed {
            id: listing.id().to_string(),
            cached: false
        }],
        "the reaper ends the listing the way a pane's close does, so every observer \
         (the change watch among them) lets go of it, after the cache removal"
    );
}

// ---- reap: negative (recently-touched, long-created listing survives) --------

#[test]
fn reaper_keeps_recently_touched_listing_even_if_created_long_ago() {
    let (engine, observer) = observed_engine();
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

    // Touched "just now" relative to the injected clock: last_accessed_ms == now. The
    // listing's `created_at` is real-now (set by the builder), but the reaper ignores
    // created_at entirely — only the fresh access stamp matters. Pick a `now` far past
    // the window so the only reason this listing survives is its fresh stamp, not a
    // small absolute clock.
    let now = 50 * window_ms;
    let listing = TestListing::new()
        .path("/live")
        .last_accessed_ms(now)
        .insert(&engine, "live");

    let reaped = engine
        .inner
        .reap_orphaned_listings_at_for(now, window_ms, &[listing.id()]);

    assert!(
        !reaped.contains(&listing.id().to_string()),
        "a just-touched listing must NOT be reaped (this is the don't-evict-live guarantee)"
    );
    assert!(listing.is_cached(), "live listing's cache entry must survive");
    assert!(
        observer.heard().is_empty(),
        "live listing's observers (its watcher among them) hear no close"
    );
}

// ---- touch() refreshes the stamp so a long-open pane is never reaped ---------

#[test]
fn touch_rescues_a_listing_that_would_otherwise_be_orphaned() {
    let engine = TestEngine::new();
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let now = 50 * window_ms;

    // Start stale (last access at the epoch → idle == now == 50x window → orphan)...
    let listing = TestListing::new()
        .path("/no/watcher")
        .last_accessed_ms(0)
        .insert(&engine, "touch");
    assert!(
        !orphan_ids(now, window_ms, std::iter::once((listing.id(), 0))).is_empty(),
        "precondition: a stamp of 0 is orphan-eligible at now = 50x window"
    );

    // ...then a live-pane access touches it (same path the read accessors take: hold
    // the cache read lock and stamp `last_accessed_ms` via `touch()`). `touch()` uses
    // the engine's REAL clock (`epoch_millis_now()`), which is a tiny value — far
    // below the injected `now` of 50x window. So the touched stamp's idle time at the
    // injected `now` is still ~50x window, which would STILL look orphaned under that
    // synthetic clock. To prove touch() works against the clock it actually uses, run
    // the reaper with the real clock + real window: the just-touched stamp is fresh.
    listing.with_listing(|cached| cached.touch());

    let reaped = engine.inner.reap_orphaned_listings_at_for(
        engine.inner.epoch_millis_now(),
        ORPHAN_IDLE_WINDOW.as_millis() as u64,
        &[listing.id()],
    );
    assert!(
        !reaped.contains(&listing.id().to_string()),
        "touch() must reset the idle clock"
    );
    assert!(listing.is_cached(), "touched listing survives the sweep");
}

// ---- the pane heartbeat keeps an idle, on-screen listing alive ----------------
//
// Regression anchor: a pane left on ~/Downloads overnight had no reads and no FS
// events for 6 h, so the reaper took its listing (and its watcher). The pane kept
// showing stale rows, new files never appeared, and every F3/F4/F5/F6 failed with
// a missing listing. The frontend now heartbeats its live listings.

#[test]
fn keep_listings_alive_rescues_an_idle_listing_from_the_reaper() {
    let engine = TestEngine::new();
    let listing = TestListing::new()
        .path("/idle/overnight")
        .last_accessed_ms(0)
        .insert(&engine, "heartbeat");

    let unknown = engine.inner.keep_listings_alive(&[listing.id().to_string()]);
    assert!(unknown.is_empty(), "a cached listing isn't reported unknown");

    let reaped = engine.inner.reap_orphaned_listings_at_for(
        engine.inner.epoch_millis_now(),
        ORPHAN_IDLE_WINDOW.as_millis() as u64,
        &[listing.id()],
    );
    assert!(reaped.is_empty(), "a heartbeated listing must survive the sweep");
    assert!(listing.is_cached());
}

#[test]
fn keep_listings_alive_names_the_listings_it_no_longer_holds() {
    let engine = TestEngine::new();
    let live = TestListing::new().path("/live").insert(&engine, "heartbeat-live");
    let gone = "heartbeat-never-existed".to_string();

    let unknown = engine.inner.keep_listings_alive(&[live.id().to_string(), gone.clone()]);

    assert_eq!(
        unknown,
        vec![gone],
        "only the missing id comes back, for the pane to re-list"
    );
}

#[test]
fn reading_a_missing_listing_is_a_typed_gone_error() {
    let engine = TestEngine::new();
    let err = engine
        .inner
        .get_file_at("read-never-existed", 0, false)
        .expect_err("no such listing");
    assert!(matches!(err, ListingLookupError::Gone { ref listing_id } if listing_id == "read-never-existed"));
}

// ---- a sweep stays inside its own test ----------------------------------------

#[test]
fn a_reaper_sweep_leaves_a_sibling_tests_listing_alone() {
    let engine = TestEngine::new();
    // A scoped sweep takes only the ids it names, even with an injected clock far
    // past the idle window that makes every listing look stale. The host's test
    // binary shares ONE engine, where an unscoped sweep would evict every other
    // test's fixture mid-assertion.
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let sibling = TestListing::new().path("/sibling").insert(&engine, "sibling");
    let mine = TestListing::new()
        .path("/mine")
        .last_accessed_ms(0)
        .insert(&engine, "mine");

    let reaped = engine
        .inner
        .reap_orphaned_listings_at_for(50 * window_ms, window_ms, &[mine.id()]);

    assert!(
        reaped.contains(&mine.id().to_string()),
        "my own stale listing is still reaped"
    );
    assert!(sibling.is_cached(), "a sibling test's listing survives my sweep");
}

// ---- the guard's own contract ------------------------------------------------

#[test]
fn guard_removes_its_listing_even_when_the_test_body_panics() {
    // Pins the panic-safety `TestListingGuard` exists for: on the host's shared
    // engine, an assertion that fails before a hand-rolled end would leak the
    // listing into every later test's view of it.
    let engine = TestEngine::new();
    let id = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let listing = TestListing::new().insert(&engine, "panic-safety");
        let id = listing.id().to_string();
        assert!(listing.is_cached());
        panic!("simulated assertion failure while the listing is cached: {id}");
    }))
    .expect_err("the closure should have panicked");
    let id = id
        .downcast_ref::<String>()
        .expect("panic payload is the formatted message")
        .rsplit(": ")
        .next()
        .expect("message ends with the listing id")
        .to_string();

    assert!(
        !engine.inner.contains(&id),
        "Drop must remove the entry on unwind, not only on the happy path"
    );
}
