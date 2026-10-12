//! Per-thread counters the row-map and path-map tests read, to pin how much
//! work an accessor does. Measurement, not engine state: the engine only bumps
//! them, and only in a test build.

/// Counts entries examined by the visibility predicate on THIS thread, so a test
/// can pin how many times an accessor walks the listing.
///
/// The wedge this guards against is a scan count, not a duration: a per-row
/// accessor that re-walks the whole listing is invisible in a unit test's wall
/// clock on a small fixture and stops the app answering IPC on a real one.
/// Counting is the only measurement that survives both.
///
/// Thread-local, because `cargo test` runs the crate's tests as threads in one
/// process and a process-wide counter would mix every concurrent listing test's
/// walking into the reading.
pub(crate) mod scan_probe {
    use std::cell::Cell;

    thread_local! {
        static EXAMINED: Cell<u64> = const { Cell::new(0) };
    }

    /// One entry passed through the visibility predicate.
    pub(crate) fn record() {
        EXAMINED.with(|c| c.set(c.get() + 1));
    }

    /// Entries this thread has examined so far. Take a `before` and an `after`
    /// and subtract.
    pub(crate) fn examined() -> u64 {
        EXAMINED.with(Cell::get)
    }
}

/// Counts entries a path lookup touched on THIS thread, and maps built, so a
/// test can pin what a batch of updates costs.
///
/// The defect these guard is a scan COUNT, invisible in a unit test's wall clock
/// on a small fixture and 64 ms of write-locked listing walking per enrichment
/// chunk on a real one. Thread-local for the same reason
/// [`scan_probe`] is: `cargo test` runs the crate's tests as threads
/// in one process, and a process-wide counter would mix every concurrent
/// listing test's work into the reading.
pub(crate) mod lookup_probe {
    use std::cell::Cell;

    thread_local! {
        static EXAMINED: Cell<u64> = const { Cell::new(0) };
        static BUILDS: Cell<u64> = const { Cell::new(0) };
    }

    /// One entry hashed into a map, or compared against a wanted path.
    pub(crate) fn record_examined() {
        EXAMINED.with(|c| c.set(c.get() + 1));
    }

    /// One map built from scratch.
    pub(crate) fn record_build() {
        BUILDS.with(|c| c.set(c.get() + 1));
    }

    /// Entries this thread has examined so far. Take a `before` and an `after`
    /// and subtract.
    pub(crate) fn examined() -> u64 {
        EXAMINED.with(Cell::get)
    }

    /// Maps this thread has built so far.
    pub(crate) fn builds() -> u64 {
        BUILDS.with(Cell::get)
    }
}
