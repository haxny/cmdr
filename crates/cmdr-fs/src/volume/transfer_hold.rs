//! [`TransferHold`]: what a volume keeps warm for as long as a transfer reads
//! from it (`Volume::hold_for_transfer`).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Something a volume keeps for a transfer reading from it, released when this
/// drops. A transfer takes one when it starts running and keeps it until it
/// settles (done, cancelled, failed, or its task dropped), so a pause between
/// files can't let the volume give up what the next file needs. Tied to the
/// transfer's own lifetime, it can't outlive the transfer.
///
/// A hold does no I/O to take or to let go (an MTP device, a dropped share):
/// it marks something kept, and the volume reads the mark.
pub struct TransferHold {
    _kept: Box<dyn Send + Sync>,
}

impl TransferHold {
    /// A hold that keeps `kept` alive, and runs its `Drop` when released.
    pub fn new(kept: impl Send + Sync + 'static) -> Self {
        Self { _kept: Box::new(kept) }
    }

    /// A hold that counts in `live` while it lives: what a test volume answers
    /// so a cell can see a transfer take and release it.
    pub fn counted(live: &Arc<AtomicUsize>) -> Self {
        struct Counted(Arc<AtomicUsize>);
        impl Drop for Counted {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        live.fetch_add(1, Ordering::SeqCst);
        Self::new(Counted(Arc::clone(live)))
    }
}

impl std::fmt::Debug for TransferHold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferHold").finish_non_exhaustive()
    }
}
