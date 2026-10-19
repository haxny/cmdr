//! A connecting backend's connection state and the generation of the session
//! it's on, in ONE atomic, so a late error from a replaced session can't mark
//! its successor lost.
//!
//! The backends whose operations are their own liveness detector (SFTP, WebDAV,
//! S3, ADB: no watcher) flip to "lost" on the first operation that fails with a
//! transport error. A call that started on the old session and fails after a
//! reconnect installed a new one would flip the HEALTHY session, drop it, and
//! hand every sibling on it an outage it never had. So a call records the
//! generation it started on, and the flip is a compare-and-swap that acts only
//! while that generation is still the installed one ([`ConnectionCell::lose`]).
//!
//! The generation is also what `Volume::connection_generation` reports: only an
//! install moves it ([`ConnectionCell::install`], [`ConnectionCell::revive`]),
//! ❌ never a plain state change, so "live on a newer generation than my call
//! started on" proves the call's error was the old session's.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};

/// A backend's own connection-state enum, as the cell stores it.
///
/// Each backend keeps its own states (SFTP rests in two more than ADB does); the
/// cell only needs to know which one means "serving" and which one a lost
/// session moves to.
pub trait CellState: Copy + Eq {
    /// The state a working session is in.
    const LIVE: Self;
    /// The state a lost session moves to, from which something is expected to
    /// retry.
    const LOST: Self;
    /// The state as the cell's low byte.
    fn to_byte(self) -> u8;
    /// The state back from the cell's low byte.
    fn from_byte(byte: u8) -> Self;
}

/// The state and the installed session's generation, packed: the low byte
/// holds the state, the rest the generation.
///
/// ❗ The only way to [`LOST`](CellState::LOST) for a failing call is
/// [`lose`](Self::lose), with the generation the call STARTED on, ❌ never a
/// fresh read of the current one after the error (that would let a replaced
/// session's loss mark its successor).
pub struct ConnectionCell<S> {
    packed: AtomicU64,
    state: PhantomData<fn() -> S>,
}

const STATE_BITS: u32 = 8;

impl<S: CellState> ConnectionCell<S> {
    /// A cell in `state`, on the session the volume was built with (generation
    /// 0).
    pub fn new(state: S) -> Self {
        Self {
            packed: AtomicU64::new(pack(state, 0)),
            state: PhantomData,
        }
    }

    /// The state right now.
    pub fn state(&self) -> S {
        unpack::<S>(self.packed.load(Ordering::Acquire)).0
    }

    /// The installed session's generation. Read it BEFORE a call to name the
    /// session the call runs on.
    pub fn generation(&self) -> u64 {
        unpack::<S>(self.packed.load(Ordering::Acquire)).1
    }

    /// Moves to `next` on whichever session is installed. Whether the state
    /// moved, so a transition several tasks noticed at once is acted on once.
    ///
    /// For the changes that aren't a failing call's verdict on its session: a
    /// reconnect attempt's outcome, a volume that's leaving, a device the OS
    /// says is gone.
    pub fn set(&self, next: S) -> bool {
        self.update(|state, generation| (state != next).then_some((next, generation)))
    }

    /// The session a call started on is lost: moves from
    /// [`LIVE`](CellState::LIVE) to [`LOST`](CellState::LOST) only while
    /// `ran_on` is still the installed generation. Whether this call flipped it,
    /// so exactly one of the calls meeting the same dead session acts on it.
    pub fn lose(&self, ran_on: u64) -> bool {
        self.update(|state, generation| (generation == ran_on && state == S::LIVE).then_some((S::LOST, generation)))
    }

    /// A new session is installed: the next generation and
    /// [`LIVE`](CellState::LIVE), in one step, so no call can see the new
    /// session under the old generation. Whether the state moved.
    ///
    /// For a backend that holds a session object and just built a fresh one,
    /// whatever the state was (an attended sign-in replaces a live session too).
    pub fn install(&self) -> bool {
        let before = self.packed.update(Ordering::AcqRel, Ordering::Acquire, |packed| {
            pack(S::LIVE, unpack::<S>(packed).1 + 1)
        });
        unpack::<S>(before).0 != S::LIVE
    }

    /// The connection came back: the next generation and
    /// [`LIVE`](CellState::LIVE), ❗ only if it wasn't live already. Whether it
    /// moved.
    ///
    /// For a backend with no session object to install, whose reconnect is a
    /// probe: a probe that finds the connection already live starts nothing
    /// new, so it moves nothing.
    pub fn revive(&self) -> bool {
        self.update(|state, generation| (state != S::LIVE).then_some((S::LIVE, generation + 1)))
    }

    /// Moves by `next`, atomically against every other transition. Whether it
    /// moved.
    fn update(&self, next: impl Fn(S, u64) -> Option<(S, u64)>) -> bool {
        self.packed
            .try_update(Ordering::AcqRel, Ordering::Acquire, |packed| {
                let (state, generation) = unpack::<S>(packed);
                next(state, generation).map(|(state, generation)| pack(state, generation))
            })
            .is_ok()
    }
}

fn pack<S: CellState>(state: S, generation: u64) -> u64 {
    (generation << STATE_BITS) | u64::from(state.to_byte())
}

fn unpack<S: CellState>(packed: u64) -> (S, u64) {
    (S::from_byte(packed as u8), packed >> STATE_BITS)
}
