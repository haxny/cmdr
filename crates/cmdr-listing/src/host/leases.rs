//! The user is waiting on this volume.
//!
//! A streaming read holds a hold for its whole body, so background work on the
//! same volume (an index scan, a transfer) backs off while the user waits. The
//! priority policy itself is the host's.

/// Hands out foreground holds.
pub trait ForegroundLeases: Send + Sync {
    /// Marks `volume_id` busy for the user until the returned hold drops.
    fn hold(&self, volume_id: &str) -> ForegroundHold;
}

/// One foreground hold. Released when it drops, on every exit: an error, a
/// cancel, a panic, or the task being dropped at shutdown.
#[must_use = "a hold that isn't kept releases at once, and background work stops yielding to the user"]
pub struct ForegroundHold {
    _guard: Option<Box<dyn Send + Sync>>,
}

impl ForegroundHold {
    /// A hold that releases by dropping `guard`.
    pub fn new(guard: impl Send + Sync + 'static) -> Self {
        Self {
            _guard: Some(Box::new(guard)),
        }
    }

    /// A hold that holds nothing.
    pub fn none() -> Self {
        Self { _guard: None }
    }
}

/// No priority policy: every hold holds nothing.
pub struct NoLeases;

impl ForegroundLeases for NoLeases {
    fn hold(&self, _volume_id: &str) -> ForegroundHold {
        ForegroundHold::none()
    }
}
