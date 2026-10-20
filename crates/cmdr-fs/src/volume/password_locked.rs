//! The one unlock seam for every password-locked container a volume serves (an
//! encrypted archive, a restic repo), so the host has one command, one dialog,
//! and one MCP tool however many kinds of lock there are.
//!
//! A host reaches it through [`Volume::as_password_locked`](super::Volume::as_password_locked),
//! ❌ never a downcast list of concrete backends: the next locked kind would
//! silently fall through it (the same reasoning as `Volume::routes_over_a_parent`).

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

use super::VolumeError;

/// What an unlock attempt did with the password.
///
/// The three differ in what a caller may conclude: only `Unlocked` proves the
/// password right, `Stored` proves nothing until the next read, and
/// `WrongPassword` kept nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UnlockOutcome {
    /// Kept without checking it (an archive): the next read decrypts, or fails
    /// with `VolumeError::NeedsPassword { wrong_attempt: true }`.
    Stored,
    /// Checked and kept: the container opened with it.
    Unlocked,
    /// Checked and refused. Nothing was kept or saved.
    WrongPassword,
}

/// A volume whose contents need a password before it can serve them.
pub trait PasswordLocked: Send + Sync {
    /// Tries `password`, keeping it in memory on success (or unchecked, for a
    /// container that can only tell on the next read). Never saves it anywhere:
    /// that's [`remember_password`](Self::remember_password), which the host calls
    /// only once it holds the answer and nobody cancelled.
    ///
    /// ❗ **Cancel-safe by contract**: a host cancels an unlock by DROPPING this
    /// future, and a dropped unlock must leave no trace. Run blocking work (a key
    /// derivation, a repo open) on the blocking pool and commit state only after
    /// it returns, in one step with NO await between the commit and the answer,
    /// so a drop lands either before anything was kept or after the answer. That
    /// also keeps every device transaction (a repo on a phone) inside the
    /// blocking task, which a drop never interrupts.
    ///
    /// `Err` is for a container that couldn't be asked at all (its disk went
    /// away); a wrong password is `Ok(WrongPassword)`, never an error.
    ///
    /// ❌ The password never reaches a log, an error, or a returned value.
    fn unlock<'a>(
        &'a self,
        password: String,
    ) -> Pin<Box<dyn Future<Output = Result<UnlockOutcome, VolumeError>> + Send + 'a>>;

    /// Files `password`, which [`unlock`](Self::unlock) just answered
    /// `Unlocked` for, in the credential store the way the dialog's Remember box
    /// asks: on saves, off only refreshes an entry already there. The default
    /// files nothing (a container that keeps no store entry, like an archive).
    ///
    /// ❗ The host calls it only AFTER the unlock answered and nobody cancelled,
    /// and awaits it to the end: a store write can't be taken back (the store has
    /// no delete), so it must never run for an unlock the person called off.
    fn remember_password<'a>(
        &'a self,
        password: String,
        remember: bool,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let _ = (password, remember);
        Box::pin(async {})
    }

    /// Forgets the password this container holds in memory (the person
    /// cancelled the prompt). A no-op when it holds none.
    fn forget_password(&self);
}
