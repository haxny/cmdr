//! Unlock commands: hand a password to whatever password-locked container a
//! path leads into (an encrypted archive, a restic repo), or forget it.
//!
//! One seam for every kind of lock: the path resolves to its volume, and the
//! volume answers through `Volume::as_password_locked` (❌ never a downcast list
//! of concrete backends). The typed [`UnlockOutcome`] says what happened: an
//! archive only STORES the password (a wrong one surfaces on the retry as the
//! typed `NeedsPassword { wrong_attempt: true }`), while a repo checks it before
//! answering `Unlocked` or `WrongPassword`.
//!
//! A repo check runs a key derivation and opens the repo, which takes seconds on
//! a slow NAS, so an unlock carries its own budget and takes a cancel token. Both
//! DROP the unlock rather than detach it (`cmdr_fs::deadline` detaches): a
//! detached unlock would keep the password and save it to the Keychain after the
//! caller was told it didn't. Dropping is sound because `PasswordLocked::unlock`
//! is cancel-safe by contract.
//!
//! The dialog cancels by naming the unlock: it picks an `unlock_id`, passes it
//! to `unlock_container`, and hands the same id to `cancel_container_unlock`
//! when the person presses Cancel mid-spinner. Two IPC calls aren't ordered, so
//! a cancel that arrives first leaves a cancelled token behind that the unlock
//! then picks up.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::IgnorePoison;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{UnlockOutcome, Volume, VolumeError};

/// How long an unlock may take. Covers a repo's key derivation plus its open
/// over a slow NAS; an archive answers at once.
const UNLOCK_BUDGET: Duration = Duration::from_secs(30);

/// The dialog's in-flight unlocks by id, plus cancels that overtook theirs.
static IN_FLIGHT: LazyLock<Mutex<HashMap<String, CancellationToken>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Past this many entries, a cancel sweeps out the cancelled ones. Those are
/// cancels whose unlock already finished (nothing will claim them) or unlocks
/// already on their way out (they hold their own clone), so dropping them is
/// harmless. Only a dialog writes here, one unlock at a time, so it's never near.
const IN_FLIGHT_SWEEP_AT: usize = 32;

/// Takes the token for `unlock_id`: the one an earlier cancel left (already
/// cancelled), or a fresh one.
fn claim_unlock(unlock_id: &str) -> CancellationToken {
    IN_FLIGHT
        .lock_ignore_poison()
        .entry(unlock_id.to_string())
        .or_default()
        .clone()
}

/// Drops the entry when its unlock ends, however it ends.
struct InFlightGuard(String);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        IN_FLIGHT.lock_ignore_poison().remove(&self.0);
    }
}

/// The ids the registry holds, for tests.
#[cfg(test)]
pub(crate) fn in_flight_unlocks() -> Vec<String> {
    IN_FLIGHT.lock_ignore_poison().keys().cloned().collect()
}

/// Why an unlock or a forget didn't reach a container. A wrong password is
/// never here: it's `Ok(UnlockOutcome::WrongPassword)`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UnlockError {
    /// `volume_id` isn't registered (an unmount race).
    VolumeNotFound {
        /// The id that no longer resolves.
        volume_id: String,
    },
    /// The path leads into nothing password-locked (a since-changed path, or a
    /// programming error).
    NotLocked,
    /// The container couldn't be asked (its disk went away mid-unlock).
    Volume {
        /// What the volume answered.
        error: VolumeError,
    },
    /// The unlock outran its budget. Nothing was kept.
    TimedOut,
    /// The caller called it off. Nothing was kept.
    Cancelled,
}

/// Tries `password` on the container at `path` on `parent_volume_id`, keeping
/// it on success and saving it to the Keychain when `remember` is set and the
/// container keeps such a thing (an archive ignores `remember`).
///
/// `path` may be the container itself OR any path inside it: both resolve to
/// the same volume, so the frontend passes whichever it has in hand.
/// `unlock_id` is the caller's name for this unlock, which
/// `cancel_container_unlock` takes.
#[tauri::command]
#[specta::specta]
pub async fn unlock_container(
    parent_volume_id: String,
    path: String,
    password: String,
    remember: bool,
    unlock_id: String,
) -> Result<UnlockOutcome, UnlockError> {
    let cancel = claim_unlock(&unlock_id);
    let _in_flight = InFlightGuard(unlock_id);
    unlock_within(
        &parent_volume_id,
        Path::new(&path),
        password,
        remember,
        &cancel,
        UNLOCK_BUDGET,
    )
    .await
}

/// Drops the unlock the caller named: it answers `Cancelled` and keeps nothing,
/// saving nothing to the Keychain. A no-op for an unlock that already answered;
/// one that hasn't started yet is cancelled the moment it does.
#[tauri::command]
#[specta::specta]
pub async fn cancel_container_unlock(unlock_id: String) {
    let mut in_flight = IN_FLIGHT.lock_ignore_poison();
    if in_flight.len() >= IN_FLIGHT_SWEEP_AT {
        in_flight.retain(|_, token| !token.is_cancelled());
    }
    in_flight.entry(unlock_id).or_default().cancel();
}

/// Forgets the password the container at `path` holds in memory (the person
/// cancelled the prompt, or the frontend is resetting state). A no-op when it
/// holds none.
#[tauri::command]
#[specta::specta]
pub async fn forget_container_password(parent_volume_id: String, path: String) -> Result<(), UnlockError> {
    let volume = resolve(&parent_volume_id, Path::new(&path)).await?;
    volume
        .as_password_locked()
        .ok_or(UnlockError::NotLocked)?
        .forget_password();
    Ok(())
}

/// The unlock itself, without the IPC wrapper, so the MCP `unlock` tool puts a
/// password in the same one place the dialog does.
///
/// ❗ The only place either surface may hand over a password. It goes straight
/// to the container and is never copied anywhere a resource, a log line, an
/// error, or a tool result can reach.
pub(crate) async fn unlock_within(
    parent_volume_id: &str,
    path: &Path,
    password: String,
    remember: bool,
    cancel: &CancellationToken,
    budget: Duration,
) -> Result<UnlockOutcome, UnlockError> {
    let unlock = async {
        let volume = resolve(parent_volume_id, path).await?;
        let locked = volume.as_password_locked().ok_or(UnlockError::NotLocked)?;
        locked
            .unlock(password, remember)
            .await
            .map_err(|error| UnlockError::Volume { error })
    };
    // Dropping the losing branch is the cancel: see the module header.
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(UnlockError::Cancelled),
        () = tokio::time::sleep(budget) => Err(UnlockError::TimedOut),
        answer = unlock => answer,
    }
}

async fn resolve(parent_volume_id: &str, path: &Path) -> Result<std::sync::Arc<dyn Volume>, UnlockError> {
    get_volume_manager()
        .resolve(parent_volume_id, path)
        .await
        .volume
        .ok_or_else(|| UnlockError::VolumeNotFound {
            volume_id: parent_volume_id.to_string(),
        })
}

#[cfg(test)]
#[path = "unlock_test.rs"]
mod unlock_test;
