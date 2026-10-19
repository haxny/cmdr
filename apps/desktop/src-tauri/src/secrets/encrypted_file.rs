//! Cocoon-encrypted file backend for Linux systems without a secret service.
//!
//! Stores secrets as a JSON map encrypted with ChaCha20-Poly1305 (via `cocoon`).
//! The encryption key is derived from `/etc/machine-id` (standard on systemd systems),
//! falling back to a fixed string where file permissions (0600) provide the primary protection.
//!
//! A new machine id (a restored home folder, a cloned VM) makes the file unreadable for good.
//! The first save or forget after that sets it aside and starts an empty store.

use super::{SecretStore, SecretStoreError};
use cocoon::Cocoon;
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

/// Mutex for serializing file access across threads.
static FILE_STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Internal serialization format: keys map to byte arrays (JSON number arrays).
#[derive(Serialize, Deserialize, Default)]
struct StoreContents(HashMap<String, Vec<u8>>);

/// Stores secrets in a cocoon-encrypted file.
pub struct EncryptedFileStore {
    path: PathBuf,
    /// The cocoon password: this machine's id, read once.
    password: Vec<u8>,
}

impl EncryptedFileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self::with_password(dir, encryption_password())
    }

    /// A store under `password` rather than this machine's id, so a test can play "the same
    /// file on another machine".
    fn with_password(dir: PathBuf, password: Vec<u8>) -> Self {
        Self {
            path: dir.join("credentials.enc"),
            password,
        }
    }
}

/// Reads `/etc/machine-id` as encryption password. Falls back to a fixed string
/// on non-systemd systems where file permissions (0600) provide the primary protection.
fn encryption_password() -> Vec<u8> {
    std::fs::read_to_string("/etc/machine-id")
        .unwrap_or_else(|_| "cmdr-credential-store".to_string())
        .trim()
        .as_bytes()
        .to_vec()
}

/// Why a store file didn't read.
#[derive(Debug)]
enum ReadFailure {
    /// The file couldn't be read at all (permissions, I/O). Nothing is known about its contents.
    Io(SecretStoreError),
    /// The file was read but doesn't decrypt under this machine's key, or doesn't parse. A home
    /// folder restored onto a new install or a cloned VM lands here: `/etc/machine-id` changed,
    /// and no key this machine has will open the file again.
    Unreadable(SecretStoreError),
}

impl From<ReadFailure> for SecretStoreError {
    fn from(failure: ReadFailure) -> Self {
        match failure {
            ReadFailure::Io(e) | ReadFailure::Unreadable(e) => e,
        }
    }
}

fn read_store(path: &PathBuf, password: &[u8]) -> Result<StoreContents, ReadFailure> {
    if !path.exists() {
        return Ok(StoreContents::default());
    }
    let encrypted = std::fs::read(path).map_err(|e| {
        ReadFailure::Io(SecretStoreError::Other(format!(
            "Could not read encrypted credential file: {}",
            e
        )))
    })?;
    let cocoon = Cocoon::new(password);
    let decrypted = cocoon.unwrap(&encrypted).map_err(|e| {
        warn!("Couldn't decrypt credential file at {}: {:?}", path.display(), e);
        ReadFailure::Unreadable(SecretStoreError::Other(format!(
            "Couldn't decrypt credential file: {:?}",
            e
        )))
    })?;
    // Pre-fix the parse-failure arm silently returned `default()` and the
    // next `set` overwrote the half-written file with empty state, losing
    // every stored secret. Surface the error so a read sees it, and a write
    // sets the file aside (`read_for_write`) rather than overwriting it.
    serde_json::from_slice(&decrypted).map_err(|e| {
        warn!("Credential file at {} couldn't be parsed: {}", path.display(), e);
        ReadFailure::Unreadable(SecretStoreError::Other(format!(
            "Couldn't parse credential file: {}",
            e
        )))
    })
}

fn write_store(path: &PathBuf, password: &[u8], contents: &StoreContents) -> Result<(), SecretStoreError> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| SecretStoreError::Other("Credential path has no parent directory".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| SecretStoreError::Other(format!("Could not create credential directory: {}", e)))?;

    let json = serde_json::to_vec(contents)
        .map_err(|e| SecretStoreError::Other(format!("Could not serialize secrets: {}", e)))?;
    let mut cocoon = Cocoon::new(password);
    let encrypted = cocoon
        .wrap(&json)
        .map_err(|e| SecretStoreError::Other(format!("Could not encrypt secrets: {:?}", e)))?;

    // Atomic write: temp file with final permissions baked in, fsync, rename.
    // Pre-fix a crash mid-write truncated the file and the next launch
    // silently dropped every stored credential.
    let temp_name = format!(".credentials.enc.cmdr-tmp-{}", uuid::Uuid::new_v4());
    let temp_path = parent.join(temp_name);

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|e| SecretStoreError::Other(format!("Could not create credential temp file: {}", e)))?;

    if let Err(e) = file.write_all(&encrypted).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(SecretStoreError::Other(format!(
            "Could not write credential file: {}",
            e
        )));
    }
    drop(file);

    std::fs::rename(&temp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        SecretStoreError::Other(format!("Could not finalize credential file: {}", e))
    })?;
    Ok(())
}

impl EncryptedFileStore {
    /// The contents a `set` or `delete` builds on. A file this machine can't read is set aside
    /// (`state_recovery`, which tells the person once) and the store starts empty, ❌ never left
    /// in the way: every later save would read it first and fail, so no password could ever be
    /// saved again. `get` and `keys` leave it alone; a read decides nothing.
    fn read_for_write(&self) -> Result<StoreContents, SecretStoreError> {
        match read_store(&self.path, &self.password) {
            Ok(contents) => Ok(contents),
            Err(ReadFailure::Io(e)) => Err(e),
            Err(ReadFailure::Unreadable(_)) => {
                // A rename that doesn't happen still starts fresh: the write that follows
                // replaces a file no key on this machine opens, and the notice says nothing
                // was kept.
                crate::state_recovery::set_aside(&self.path, crate::state_recovery::StateFile::Credentials);
                Ok(StoreContents::default())
            }
        }
    }
}

impl SecretStore for EncryptedFileStore {
    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let mut contents = self.read_for_write()?;
        contents.0.insert(key.to_string(), value.to_vec());
        write_store(&self.path, &self.password, &contents)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let contents = read_store(&self.path, &self.password)?;
        contents
            .0
            .get(key)
            .cloned()
            .ok_or_else(|| SecretStoreError::NotFound(format!("No secret found for key: {}", key)))
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        let mut contents = self.read_for_write()?;
        if contents.0.remove(key).is_none() {
            return Err(SecretStoreError::NotFound(format!("No secret found for key: {}", key)));
        }
        write_store(&self.path, &self.password, &contents)
    }

    fn keys(&self) -> Result<Vec<String>, SecretStoreError> {
        let _lock = FILE_STORE_LOCK
            .lock()
            .map_err(|e| SecretStoreError::Other(format!("Lock error: {}", e)))?;
        Ok(read_store(&self.path, &self.password)?.0.into_keys().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let password = encryption_password();
        let data = b"test data for encryption";
        let mut cocoon = Cocoon::new(&password);
        let encrypted = cocoon.wrap(data).expect("wrap should succeed");
        let decrypted = Cocoon::new(&password)
            .unwrap(&encrypted)
            .expect("unwrap should succeed");
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_store_contents_serde_roundtrip() {
        let mut contents = StoreContents::default();
        contents.0.insert("key1".to_string(), b"hello".to_vec());
        contents.0.insert("key2".to_string(), b"world".to_vec());

        let json = serde_json::to_vec(&contents).unwrap();
        let parsed: StoreContents = serde_json::from_slice(&json).unwrap();

        assert_eq!(parsed.0.get("key1").unwrap(), b"hello");
        assert_eq!(parsed.0.get("key2").unwrap(), b"world");
    }

    #[test]
    fn test_read_store_missing_file() {
        let contents = read_store(&PathBuf::from("/tmp/nonexistent-cmdr-test-file.enc"), b"any").unwrap();
        assert!(contents.0.is_empty());
    }

    // ── A file this machine can't read (#537) ────────────────────────────

    use crate::state_recovery::{StateFile, StateRecovery, testing as recovery};
    use crate::test_support::TestDir;

    /// `credentials.enc` written under one machine's key, then opened under another's: a home
    /// folder restored onto a new install, or a cloned VM.
    fn moved_to_a_new_machine(dir: &TestDir) -> EncryptedFileStore {
        EncryptedFileStore::with_password(dir.to_path_buf(), b"old-machine-id".to_vec())
            .set("smb://naspolya/photos", b"anna\0secret")
            .expect("the old machine saves");
        EncryptedFileStore::with_password(dir.to_path_buf(), b"new-machine-id".to_vec())
    }

    #[test]
    fn a_round_trip_under_this_machines_key() {
        let dir = TestDir::new("encrypted_file_round_trip");
        let store = EncryptedFileStore::new(dir.to_path_buf());
        store.set("ai.apiKey.openai", b"sk-test").expect("set");
        assert_eq!(store.get("ai.apiKey.openai").expect("get"), b"sk-test");
    }

    #[test]
    fn saving_on_a_new_machine_sets_the_old_file_aside_and_starts_a_new_store() {
        let notices = recovery::capture_recoveries();
        let dir = TestDir::new("encrypted_file_new_machine_set");
        let store = moved_to_a_new_machine(&dir);

        store
            .set("smb://naspolya/photos", b"anna\0typed-again")
            .expect("a save works again");

        assert_eq!(store.get("smb://naspolya/photos").expect("get"), b"anna\0typed-again");
        let kept = recovery::kept_copies(&store.path);
        assert_eq!(kept.len(), 1, "the unreadable file is kept, not deleted");
        assert_eq!(
            notices.recoveries(),
            vec![StateRecovery {
                file: StateFile::Credentials,
                kept_copy: Some(kept[0].to_string_lossy().into_owned()),
            }]
        );
    }

    #[test]
    fn forgetting_on_a_new_machine_sets_the_old_file_aside_too() {
        let notices = recovery::capture_recoveries();
        let dir = TestDir::new("encrypted_file_new_machine_delete");
        let store = moved_to_a_new_machine(&dir);

        assert!(matches!(
            store.delete("smb://naspolya/photos"),
            Err(SecretStoreError::NotFound(_))
        ));

        assert_eq!(recovery::kept_copies(&store.path).len(), 1);
        assert_eq!(notices.recoveries().len(), 1);
        store
            .set("smb://naspolya/photos", b"anna\0new")
            .expect("and the next save works");
        assert_eq!(notices.recoveries().len(), 1, "told once");
    }

    #[test]
    fn a_file_that_decrypts_but_doesnt_parse_is_set_aside_on_save() {
        let _notices = recovery::capture_recoveries();
        let dir = TestDir::new("encrypted_file_unparseable");
        let store = EncryptedFileStore::with_password(dir.to_path_buf(), b"this-machine".to_vec());
        let garbage = Cocoon::new(b"this-machine").wrap(b"{\"trunc").expect("wrap");
        std::fs::write(&store.path, garbage).expect("write");

        store.set("license.key", b"{}").expect("a save works again");

        assert_eq!(recovery::kept_copies(&store.path).len(), 1);
    }

    #[test]
    fn reading_on_a_new_machine_answers_an_error_and_leaves_the_file_alone() {
        // A read decides nothing: it doesn't destroy what it can't read, and it stays quiet.
        let notices = recovery::capture_recoveries();
        let dir = TestDir::new("encrypted_file_new_machine_get");
        let store = moved_to_a_new_machine(&dir);

        assert!(matches!(
            store.get("smb://naspolya/photos"),
            Err(SecretStoreError::Other(_))
        ));
        assert!(store.keys().is_err());

        assert!(store.path.exists());
        assert!(recovery::kept_copies(&store.path).is_empty());
        assert!(notices.recoveries().is_empty());
    }
}
