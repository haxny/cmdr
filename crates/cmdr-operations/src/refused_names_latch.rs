//! The operation's "Skip all" for names the destination refuses:
//! [`RefusedNames`]. A leaf the state owns, so `refused_name.rs` (which reads
//! the state) and `state.rs` (which holds the latch) stay out of a cycle.

use std::sync::atomic::{AtomicBool, Ordering};

/// The operation's "Skip all" for refused names: once a person picks it, no
/// later refused name in this operation asks again.
// DEFAULT-OK: unlatched is the truth before anyone has answered anything.
#[derive(Default)]
pub(crate) struct RefusedNames {
    skip_all: AtomicBool,
}

impl RefusedNames {
    pub(crate) fn skips_all(&self) -> bool {
        self.skip_all.load(Ordering::Acquire)
    }

    pub(crate) fn latch_skip_all(&self) {
        self.skip_all.store(true, Ordering::Release);
    }
}
