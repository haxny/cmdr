//! The unlock seam: a plain backend isn't locked, and the outcome crosses IPC
//! under stable wire names the frontend and the MCP tool branch on.

use super::{PasswordLocked, UnlockOutcome};
use crate::volume::{InMemoryVolume, Volume};

#[test]
fn a_backend_that_locks_nothing_answers_no_seam() {
    assert!(InMemoryVolume::new("V").as_password_locked().is_none());
}

#[test]
fn every_outcome_has_its_own_wire_name() {
    let wire = |outcome: UnlockOutcome| serde_json::to_value(outcome).expect("the outcome serializes");
    assert_eq!(wire(UnlockOutcome::Stored), "stored");
    assert_eq!(wire(UnlockOutcome::Unlocked), "unlocked");
    assert_eq!(wire(UnlockOutcome::WrongPassword), "wrongPassword");
}

/// The trait stays object-safe: the host only ever holds it as `&dyn`.
#[test]
fn the_seam_is_object_safe() {
    fn takes(_: Option<&dyn PasswordLocked>) {}
    takes(None);
}
