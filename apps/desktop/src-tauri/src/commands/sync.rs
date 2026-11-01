//! IPC for Synchronize folders (⇧F12). Thin: the work is `crate::sync` and `cmdr-sync`.

use cmdr_sync::{KeepPolicy, KeepRefusal, RootRef, SyncOptions, SyncProfile, check_keep_policy};

use crate::sync::profile_store::{
    NameTaken, PROFILES, ProfileEditRefusal, ProfileEntry, ProfileMove, ProfileSaveRefusal, WriteFailed, delete_in,
    move_in, remember_options_in, rename_in, save_in,
};

/// The saved profiles in the person's order. A profile that can't be used is listed
/// with why, never dropped.
#[tauri::command]
#[specta::specta]
pub fn get_sync_profiles(_app: tauri::AppHandle) -> Vec<ProfileEntry> {
    PROFILES.list()
}

/// Saves a profile at the end of the list; one with the same name (trimmed, any case) is
/// replaced where it stands. Answers the list after, or why nothing was saved
/// (`writeFailed`: it is in memory only).
#[tauri::command]
#[specta::specta]
pub fn save_sync_profile(app: tauri::AppHandle, profile: SyncProfile) -> Result<Vec<ProfileEntry>, ProfileSaveRefusal> {
    let mut refused = None;
    let entries = PROFILES.edit(&app, "a save", |list| match save_in(list, profile) {
        Ok(()) => true,
        Err(refusal) => {
            refused = Some(refusal);
            false
        }
    });
    match (refused, entries) {
        (Some(refusal), _) => Err(refusal),
        (None, Ok(entries)) => Ok(entries),
        (None, Err(WriteFailed)) => Err(ProfileSaveRefusal::WriteFailed),
    }
}

/// Deletes a profile by id. No-op when it isn't there. Answers the list after.
#[tauri::command]
#[specta::specta]
pub fn delete_sync_profile(app: tauri::AppHandle, id: String) -> Result<Vec<ProfileEntry>, ProfileEditRefusal> {
    PROFILES
        .edit(&app, "a delete", |list| delete_in(list, &id))
        .map_err(|WriteFailed| ProfileEditRefusal::WriteFailed)
}

/// Renames a profile in place. A name another profile has is refused (`nameTaken`); the
/// dialog asks whether to replace it and saves instead. No-op for an unknown id or an empty
/// name. Answers the list after.
#[tauri::command]
#[specta::specta]
pub fn rename_sync_profile(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<Vec<ProfileEntry>, ProfileEditRefusal> {
    let mut taken = false;
    let entries = PROFILES.edit(&app, "a rename", |list| match rename_in(list, &id, &name) {
        Ok(changed) => changed,
        Err(NameTaken) => {
            taken = true;
            false
        }
    });
    if taken {
        return Err(ProfileEditRefusal::NameTaken);
    }
    entries.map_err(|WriteFailed| ProfileEditRefusal::WriteFailed)
}

/// Swaps a profile with its neighbor. No-op at that end of the list or for an unknown id.
/// Answers the list after.
#[tauri::command]
#[specta::specta]
pub fn move_sync_profile(
    app: tauri::AppHandle,
    id: String,
    direction: ProfileMove,
) -> Result<Vec<ProfileEntry>, ProfileEditRefusal> {
    PROFILES
        .edit(&app, "a move", |list| move_in(list, &id, direction))
        .map_err(|WriteFailed| ProfileEditRefusal::WriteFailed)
}

/// The options the dialog last ran with for this ordered pair of roots.
#[tauri::command]
#[specta::specta]
pub fn get_sync_last_options(_app: tauri::AppHandle, left: RootRef, right: RootRef) -> Option<SyncOptions> {
    PROFILES.last_options(&left, &right)
}

/// Remembers the dialog's options for this pair of roots (the newest 50 pairs are kept).
#[tauri::command]
#[specta::specta]
pub fn save_sync_last_options(
    app: tauri::AppHandle,
    left: RootRef,
    right: RootRef,
    options: SyncOptions,
) -> Result<(), ProfileEditRefusal> {
    PROFILES
        .edit(&app, "remembering options", |list| {
            remember_options_in(list, &left, &right, &options)
        })
        .map(|_| ())
        .map_err(|WriteFailed| ProfileEditRefusal::WriteFailed)
}

/// Whether the versions folder these settings name can be used, so Settings can say so
/// next to the field. `None` is fine. A custom folder inside a synchronized folder is
/// only known to be wrong once a run has roots.
#[tauri::command]
#[specta::specta]
pub fn check_sync_keep_policy(policy: KeepPolicy) -> Option<KeepRefusal> {
    check_keep_policy(&policy).err()
}
