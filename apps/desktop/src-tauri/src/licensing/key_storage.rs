//! Where the license key lives: the secret store (the Keychain on macOS), never `license.json`.
//!
//! The key's payload carries the buyer's email, and the key (or its short code) is what activates
//! a license, so it's a secret like an SMB password or an API key. `license.json` keeps only
//! non-secret state: the cached server verdict, timers, and [`FILE_KEY_IN_SECRET_STORE`], the
//! marker that says a key is waiting in the secret store. The marker is what keeps a Personal
//! install from ever touching the Keychain.
//!
//! Older versions wrote the key into `license.json` in plain text. [`migrate`] moves it on launch,
//! and only removes it from the file once the secret store gave the same bytes back. See
//! `DETAILS.md` § Key storage.

use crate::secrets::{SecretStore, SecretStoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri_plugin_store::StoreExt;

/// The licensing store file, under the app data dir.
pub(super) const LICENSE_FILE: &str = "license.json";

/// The secret-store entry holding [`StoredLicense`] as JSON.
pub(super) const SECRET_KEY: &str = "license.key";

/// Where versions before the move kept the key, in plain text. Read only to migrate it (or to use
/// it while a migration keeps failing), and written by nothing.
const FILE_KEY_LEGACY_LICENSE: &str = "license_key";
const FILE_KEY_LEGACY_SHORT_CODE: &str = "license_short_code";

/// `true` in `license.json` when the key is in the secret store.
const FILE_KEY_IN_SECRET_STORE: &str = "license_key_in_secret_store";

/// The secret part of a license: the full key and the short code it was activated with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredLicense {
    pub license_key: String,
    pub short_code: Option<String>,
}

/// The slice of `license.json` this module touches: the tauri store in production, a map in tests.
pub(super) trait LicenseFile {
    fn get(&self, key: &str) -> Option<Value>;
    fn set(&self, key: &str, value: Value);
    fn delete(&self, key: &str);
}

impl<R: tauri::Runtime> LicenseFile for tauri_plugin_store::Store<R> {
    fn get(&self, key: &str) -> Option<Value> {
        tauri_plugin_store::Store::get(self, key)
    }

    fn set(&self, key: &str, value: Value) {
        tauri_plugin_store::Store::set(self, key, value);
    }

    fn delete(&self, key: &str) {
        // `false` only means the key wasn't there, which is the goal anyway.
        let _ = tauri_plugin_store::Store::delete(self, key);
    }
}

/// Moves a license key an older version left in `license.json` in plain text into the secret
/// store. Runs once per launch, before the first status check; a no-op once moved. A failure
/// keeps the key in the file (the license keeps working from there) and retries next launch.
pub fn move_license_key_into_secret_store(app: &tauri::AppHandle) {
    let store = match app.store(LICENSE_FILE) {
        Ok(store) => store,
        Err(e) => {
            log::warn!("Couldn't open {LICENSE_FILE} to check for a license key to move: {e}");
            return;
        }
    };
    match migrate(&*store, crate::secrets::store()) {
        MigrationOutcome::NothingToMigrate => {}
        MigrationOutcome::Migrated => log::info!("Moved the license key from {LICENSE_FILE} into the secret store"),
        MigrationOutcome::KeptInFile(reason) => log::warn!(
            "Couldn't move the license key into the secret store, keeping it in {LICENSE_FILE} and retrying next launch: {reason}"
        ),
    }
}

/// Removes the stored key from the secret store and `license.json`. Called by `reset_license`.
pub(super) fn clear_stored_license(app: &tauri::AppHandle) {
    if let Ok(store) = app.store(LICENSE_FILE) {
        clear(&*store, crate::secrets::store());
    }
}

/// What [`migrate`] did, for the log.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MigrationOutcome {
    /// No key in `license.json`: nothing to move.
    NothingToMigrate,
    /// The key is in the secret store and gone from the file.
    Migrated,
    /// The secret store didn't take the key, so the file stays exactly as it was and the next
    /// launch tries again.
    KeptInFile(String),
}

/// Moves a key an older version left in `license.json` into the secret store.
///
/// Never loses a license: the file loses the key only after the secret store returned exactly the
/// bytes written. Any failure leaves the file untouched, [`load`] keeps reading the key from it,
/// and the next launch retries.
pub(super) fn migrate(file: &impl LicenseFile, secrets: &dyn SecretStore) -> MigrationOutcome {
    let Some(license) = license_in_file(file) else {
        return MigrationOutcome::NothingToMigrate;
    };
    match write_and_read_back(secrets, &license) {
        Ok(()) => {
            mark_in_secret_store(file);
            MigrationOutcome::Migrated
        }
        Err(e) => MigrationOutcome::KeptInFile(e.to_string()),
    }
}

/// Stores a newly activated license. On `Err`, the file is unchanged.
pub(super) fn save(
    file: &impl LicenseFile,
    secrets: &dyn SecretStore,
    license: &StoredLicense,
) -> Result<(), SecretStoreError> {
    write_and_read_back(secrets, license)?;
    mark_in_secret_store(file);
    Ok(())
}

/// The stored license, if any. Touches the secret store only when the marker says a key is there.
///
/// A key still in the file wins: it's either a migration that hasn't succeeded yet, or a newer key
/// an older version wrote after a downgrade, which the next launch's migration moves over.
pub(super) fn load(
    file: &impl LicenseFile,
    secrets: &dyn SecretStore,
) -> Result<Option<StoredLicense>, SecretStoreError> {
    if let Some(license) = license_in_file(file) {
        return Ok(Some(license));
    }
    if file.get(FILE_KEY_IN_SECRET_STORE).and_then(|v| v.as_bool()) != Some(true) {
        return Ok(None);
    }
    match secrets.get(SECRET_KEY) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| SecretStoreError::Other(format!("The stored license entry doesn't parse: {e}"))),
        Err(SecretStoreError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Removes the license from both places. A secret-store failure is logged, not returned: the
/// marker goes either way, so the app reads as unlicensed, and the next `save` overwrites the
/// leftover entry.
pub(super) fn clear(file: &impl LicenseFile, secrets: &dyn SecretStore) {
    match secrets.delete(SECRET_KEY) {
        Ok(()) | Err(SecretStoreError::NotFound(_)) => {}
        Err(e) => log::warn!("Couldn't remove the license key from the secret store: {e}"),
    }
    file.delete(FILE_KEY_IN_SECRET_STORE);
    file.delete(FILE_KEY_LEGACY_LICENSE);
    file.delete(FILE_KEY_LEGACY_SHORT_CODE);
}

/// A key an older version wrote into the file, if there is one.
fn license_in_file(file: &impl LicenseFile) -> Option<StoredLicense> {
    let license_key = file.get(FILE_KEY_LEGACY_LICENSE)?.as_str()?.to_string();
    let short_code = file
        .get(FILE_KEY_LEGACY_SHORT_CODE)
        .and_then(|v| v.as_str().map(str::to_string));
    Some(StoredLicense {
        license_key,
        short_code,
    })
}

/// Writes the entry, then reads it back: a locked keyring can accept a write it never keeps.
fn write_and_read_back(secrets: &dyn SecretStore, license: &StoredLicense) -> Result<(), SecretStoreError> {
    let bytes = serde_json::to_vec(license)
        .map_err(|e| SecretStoreError::Other(format!("Couldn't serialize the license entry: {e}")))?;
    secrets.set(SECRET_KEY, &bytes)?;
    if secrets.get(SECRET_KEY)? != bytes {
        return Err(SecretStoreError::Other(
            "The secret store gave back different bytes than it was given".to_string(),
        ));
    }
    Ok(())
}

/// Records that the key is in the secret store and drops any plaintext copy from the file.
fn mark_in_secret_store(file: &impl LicenseFile) {
    file.set(FILE_KEY_IN_SECRET_STORE, Value::Bool(true));
    file.delete(FILE_KEY_LEGACY_LICENSE);
    file.delete(FILE_KEY_LEGACY_SHORT_CODE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ignore_poison::IgnorePoison;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct MapFile(Mutex<HashMap<String, Value>>);

    impl LicenseFile for MapFile {
        fn get(&self, key: &str) -> Option<Value> {
            self.0.lock_ignore_poison().get(key).cloned()
        }
        fn set(&self, key: &str, value: Value) {
            self.0.lock_ignore_poison().insert(key.to_string(), value);
        }
        fn delete(&self, key: &str) {
            self.0.lock_ignore_poison().remove(key);
        }
    }

    impl MapFile {
        fn snapshot(&self) -> HashMap<String, Value> {
            self.0.lock_ignore_poison().clone()
        }
    }

    /// An in-memory secret store that can refuse writes, hand back other bytes than it was given,
    /// and counts every call.
    #[derive(Default)]
    struct FakeSecrets {
        entries: Mutex<HashMap<String, Vec<u8>>>,
        failing_writes: AtomicUsize,
        corrupt_reads: bool,
        calls: AtomicUsize,
    }

    impl FakeSecrets {
        fn failing_next_writes(count: usize) -> Self {
            let fake = Self::default();
            fake.failing_writes.store(count, Ordering::SeqCst);
            fake
        }
        fn stored(&self) -> Option<StoredLicense> {
            let entries = self.entries.lock_ignore_poison();
            entries
                .get(SECRET_KEY)
                .map(|bytes| serde_json::from_slice(bytes).expect("the fake holds only what save wrote"))
        }
    }

    impl SecretStore for FakeSecrets {
        fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self
                .failing_writes
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(SecretStoreError::AccessDenied("user denied".to_string()));
            }
            self.entries
                .lock_ignore_poison()
                .insert(key.to_string(), value.to_vec());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.corrupt_reads {
                return Ok(b"something else".to_vec());
            }
            self.entries
                .lock_ignore_poison()
                .get(key)
                .cloned()
                .ok_or_else(|| SecretStoreError::NotFound(key.to_string()))
        }
        fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entries
                .lock_ignore_poison()
                .remove(key)
                .map(|_| ())
                .ok_or_else(|| SecretStoreError::NotFound(key.to_string()))
        }
    }

    const KEY: &str = "eyJlbWFpbCI6ImJ1eWVyQGV4YW1wbGUuY29tIn0=.c2lnbmF0dXJl";

    fn file_from_an_older_version() -> MapFile {
        let file = MapFile::default();
        file.set(FILE_KEY_LEGACY_LICENSE, serde_json::json!(KEY));
        file.set(FILE_KEY_LEGACY_SHORT_CODE, serde_json::json!("CMDR-AAAA-BBBB-CCCC"));
        file.set("cached_license_status", serde_json::json!({ "status": "active" }));
        file
    }

    fn the_license() -> StoredLicense {
        StoredLicense {
            license_key: KEY.to_string(),
            short_code: Some("CMDR-AAAA-BBBB-CCCC".to_string()),
        }
    }

    #[test]
    fn migration_moves_the_key_out_of_the_file_into_the_secret_store() {
        let file = file_from_an_older_version();
        let secrets = FakeSecrets::default();

        assert_eq!(migrate(&file, &secrets), MigrationOutcome::Migrated);

        assert_eq!(secrets.stored(), Some(the_license()));
        assert_eq!(
            file.get(FILE_KEY_LEGACY_LICENSE),
            None,
            "the plaintext key must leave the file"
        );
        assert_eq!(file.get(FILE_KEY_LEGACY_SHORT_CODE), None);
        assert_eq!(file.get(FILE_KEY_IN_SECRET_STORE), Some(serde_json::json!(true)));
        assert!(
            file.get("cached_license_status").is_some(),
            "non-secret state stays in the file"
        );
        assert_eq!(load(&file, &secrets).unwrap(), Some(the_license()));
    }

    #[test]
    fn migration_keeps_the_file_untouched_when_the_secret_store_refuses_the_write() {
        let file = file_from_an_older_version();
        let before = file.snapshot();
        let secrets = FakeSecrets::failing_next_writes(1);

        assert!(matches!(migrate(&file, &secrets), MigrationOutcome::KeptInFile(_)));

        assert_eq!(file.snapshot(), before, "a failed migration must not touch the file");
        assert_eq!(
            load(&file, &secrets).unwrap(),
            Some(the_license()),
            "the license still works from the file"
        );
    }

    #[test]
    fn migration_keeps_the_file_when_the_read_back_differs() {
        let file = file_from_an_older_version();
        let before = file.snapshot();
        let secrets = FakeSecrets {
            corrupt_reads: true,
            ..FakeSecrets::default()
        };

        assert!(matches!(migrate(&file, &secrets), MigrationOutcome::KeptInFile(_)));
        assert_eq!(file.snapshot(), before);
    }

    #[test]
    fn a_failed_migration_succeeds_on_the_next_launch() {
        let file = file_from_an_older_version();
        let secrets = FakeSecrets::failing_next_writes(1);

        assert!(matches!(migrate(&file, &secrets), MigrationOutcome::KeptInFile(_)));
        assert_eq!(migrate(&file, &secrets), MigrationOutcome::Migrated);

        assert_eq!(secrets.stored(), Some(the_license()));
        assert_eq!(file.get(FILE_KEY_LEGACY_LICENSE), None);
    }

    #[test]
    fn migration_without_a_key_in_the_file_never_touches_the_secret_store() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();

        assert_eq!(migrate(&file, &secrets), MigrationOutcome::NothingToMigrate);
        assert_eq!(secrets.calls.load(Ordering::SeqCst), 0);
    }

    /// A Personal install has no marker, so status checks (which run often) never reach the
    /// Keychain, and never risk a Keychain prompt.
    #[test]
    fn loading_without_a_license_never_touches_the_secret_store() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();

        assert_eq!(load(&file, &secrets).unwrap(), None);
        assert_eq!(secrets.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn save_puts_the_key_in_the_secret_store_and_none_in_the_file() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();

        save(&file, &secrets, &the_license()).unwrap();

        assert_eq!(secrets.stored(), Some(the_license()));
        assert_eq!(file.get(FILE_KEY_LEGACY_LICENSE), None);
        assert_eq!(file.get(FILE_KEY_IN_SECRET_STORE), Some(serde_json::json!(true)));
        assert_eq!(load(&file, &secrets).unwrap(), Some(the_license()));
    }

    #[test]
    fn a_failed_save_changes_nothing() {
        let file = MapFile::default();
        let secrets = FakeSecrets::failing_next_writes(1);

        assert!(save(&file, &secrets, &the_license()).is_err());
        assert!(file.snapshot().is_empty());
    }

    /// After a downgrade, an older version can only have written its key to the file, so that's
    /// the newer one.
    #[test]
    fn a_key_back_in_the_file_wins_over_the_secret_store() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();
        save(
            &file,
            &secrets,
            &StoredLicense {
                license_key: "older.key".to_string(),
                short_code: None,
            },
        )
        .unwrap();
        file.set(FILE_KEY_LEGACY_LICENSE, serde_json::json!(KEY));
        file.set(FILE_KEY_LEGACY_SHORT_CODE, serde_json::json!("CMDR-AAAA-BBBB-CCCC"));

        assert_eq!(load(&file, &secrets).unwrap(), Some(the_license()));
        assert_eq!(migrate(&file, &secrets), MigrationOutcome::Migrated);
        assert_eq!(secrets.stored(), Some(the_license()));
    }

    #[test]
    fn a_marker_with_no_secret_reads_as_no_license() {
        let file = MapFile::default();
        file.set(FILE_KEY_IN_SECRET_STORE, serde_json::json!(true));

        assert_eq!(load(&file, &FakeSecrets::default()).unwrap(), None);
    }

    #[test]
    fn clear_removes_the_license_from_both_places() {
        let file = file_from_an_older_version();
        let secrets = FakeSecrets::default();
        save(&file, &secrets, &the_license()).unwrap();
        file.set(FILE_KEY_LEGACY_LICENSE, serde_json::json!(KEY));

        clear(&file, &secrets);

        assert_eq!(secrets.stored(), None);
        assert_eq!(file.get(FILE_KEY_LEGACY_LICENSE), None);
        assert_eq!(file.get(FILE_KEY_LEGACY_SHORT_CODE), None);
        assert_eq!(file.get(FILE_KEY_IN_SECRET_STORE), None);
        assert_eq!(load(&file, &secrets).unwrap(), None);
    }
}
