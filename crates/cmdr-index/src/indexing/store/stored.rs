//! A volume's index read straight off its database file, with no live instance.
//!
//! For answering about a volume that isn't indexing right now: a saved share that
//! isn't connected, an ejected drive. Its last-scan facts (`Index::status`), and
//! the per-path reads (`read/queries.rs`'s `IndexReader`), over the same file.
//!
//! - **Read-only, and never repaired.** `SQLITE_OPEN_READ_ONLY`, no `create_tables`,
//!   no schema stamp. A database this build can't read (another schema, not a
//!   database at all) is REFUSED, typed, and left exactly as it was: the rebuild
//!   is a live start's call (`IndexStore::open`), ❌ never a reader's.
//! - **Nothing is stood up.** No instance is registered and no read pool is
//!   installed, so the volume keeps reading as not indexed everywhere else
//!   ("registered is not indexed", `VolumeIndexStatus::enabled`).
//! - **One connection per [`StoredIndex`], closed when it drops.** ❌ Never cached
//!   per thread: a stop retires cached connections so the file can go
//!   (`crates/cmdr-index/DETAILS.md` § A stop retires too), and one held here
//!   would pin a database the retention sweep has removed.

use std::path::Path;

use rusqlite::Connection;

use super::{IndexStatus, IndexStore, SCHEMA_VERSION, register_platform_case_collation};

/// Why a volume's database can't be read as an index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoredIndexRefusal {
    /// There's no database for this volume (or no data dir to look in).
    Missing,
    /// The file was written with a schema this build doesn't read, older or newer.
    /// Not repaired here: the next live start rebuilds it.
    OtherSchema {
        /// The schema version the file records, `None` when it records none.
        found: Option<String>,
    },
    /// It exists but couldn't be opened or read: locked past the busy timeout, an
    /// I/O error, or not an index database at all. Kept as it is.
    Unreadable,
}

/// An open, read-only view of one volume's index database.
pub(crate) struct StoredIndex {
    conn: Connection,
}

impl StoredIndex {
    /// Open the database at `db_path` for reading, refusing one this build can't
    /// read rather than touching it.
    pub(crate) fn open(db_path: &Path) -> Result<Self, StoredIndexRefusal> {
        // Checked first: a read-only open of a missing file fails anyway, but as an
        // I/O error that would read as `Unreadable`.
        if !db_path.is_file() {
            return Err(StoredIndexRefusal::Missing);
        }
        let conn = IndexStore::open_read_connection(db_path).map_err(|_| StoredIndexRefusal::Unreadable)?;
        register_platform_case_collation(&conn).map_err(|_| StoredIndexRefusal::Unreadable)?;
        match IndexStore::read_meta_value(&conn, "schema_version") {
            Ok(Some(version)) if version == SCHEMA_VERSION => Ok(Self { conn }),
            Ok(found) => Err(StoredIndexRefusal::OtherSchema { found }),
            // No `meta` table: a file at the path that isn't an index (yet).
            Err(_) => Err(StoredIndexRefusal::Unreadable),
        }
    }

    /// The configured drive-index database for `volume_id`.
    pub(crate) fn open_for_volume(volume_id: &str) -> Result<Self, StoredIndexRefusal> {
        let dirs = crate::volume_files::StoreDirs::configured().map_err(|_| StoredIndexRefusal::Missing)?;
        Self::open(&dirs.db_path(crate::volume_files::VolumeStore::Index, volume_id))
    }

    /// The persisted facts about the volume's last walk: where it was mounted
    /// (`volume_path`), when that walk finished, and its counts.
    pub(crate) fn last_scan(&self) -> Result<IndexStatus, StoredIndexRefusal> {
        let read = |key| IndexStore::read_meta_value(&self.conn, key).map_err(|_| StoredIndexRefusal::Unreadable);
        Ok(IndexStatus {
            schema_version: read("schema_version")?,
            volume_path: read("volume_path")?,
            scan_completed_at: read("scan_completed_at")?,
            scan_duration_ms: read("scan_duration_ms")?,
            total_entries: read("total_entries")?,
            total_physical_bytes: read("total_physical_bytes")?,
            last_event_id: read("last_event_id")?,
        })
    }

    /// The read-only connection, for the per-path queries a live read pool also
    /// serves. Lives exactly as long as this value.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
#[path = "stored_tests.rs"]
mod tests;
