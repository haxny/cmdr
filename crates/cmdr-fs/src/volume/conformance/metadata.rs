//! What a copy promises about the metadata beyond mtime: a destination keeps
//! each class it says it stores, and a source reports each class it says it
//! reports, matching what it lists.
//!
//! Each backend passes the classes it claims, explicitly, so a backend can't
//! claim a class it doesn't honor. The classes a listing can show (creation
//! date, mode) are asserted here; the flags and the xattrs have no listing field,
//! so a backend that stores them pins them in its own suite. The contract:
//! `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md`
//! § "Copies keep the source's metadata".

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::modification_date::SOURCE_DATE_SECS;
use crate::metadata::{CarriedMetadata, ClassOutcome, MetadataClasses};
use crate::volume::{StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};

/// The creation date the assertions hand a destination, in Unix seconds:
/// 2015-03-01 00:00:00 UTC. Years before [`SOURCE_DATE_SECS`], the way a real
/// file is born before it's last edited, so a destination that sets only mtime
/// (APFS then drags birth down to it) or stamps "now" can't pass.
pub const BIRTH_SECS: u64 = 1_425_168_000;

/// The mode the assertions hand a destination: owner read and write only. Any
/// umask leaves it alone, so a destination that folds a routed mode and one
/// that stores it exactly both land it as is.
const SOURCE_MODE: u32 = 0o600;

/// The mode a folder gets from [`assert_apply_metadata_keeps`], for the same
/// reason as [`SOURCE_MODE`].
const FOLDER_MODE: u32 = 0o700;

/// A copy onto `volume` keeps each class in `kept` that a listing can show.
///
/// Streams a few bytes into `dest` through [`Volume::write_from_stream`] from a
/// source born at [`BIRTH_SECS`], last modified at [`SOURCE_DATE_SECS`], with
/// mode `0600`, then insists the destination lists the creation date (when
/// `kept` holds [`MetadataClasses::CREATED`]) and the mode (when it holds
/// [`MetadataClasses::MODE`]), and still the modification date. `dest` must not
/// exist yet.
pub async fn assert_write_from_stream_keeps_metadata(volume: &dyn Volume, dest: &Path, kept: MetadataClasses) {
    assert!(
        !volume.exists(dest).await,
        "fixture precondition: {} must not exist yet",
        dest.display()
    );
    let content = b"bytes from a file born in 2015\n".to_vec();
    let length = content.len() as u64;
    let written = volume
        .write_from_stream(
            dest,
            WriteMode::CreateNew,
            StreamLength::Known(length),
            Box::new(CarryingSource::new(content)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await;
    assert_eq!(
        written.as_ref().ok(),
        Some(&length),
        "the write onto {} must land every byte; got {written:?}",
        dest.display()
    );

    let listed = volume
        .get_metadata(dest)
        .await
        .unwrap_or_else(|e| panic!("{} must be stattable after the write, got {e:?}", dest.display()));
    assert_eq!(
        listed.modified_at,
        Some(SOURCE_DATE_SECS),
        "a copy onto {} must still keep the source's modification date",
        dest.display()
    );
    if kept.contains(MetadataClasses::CREATED) {
        assert_eq!(
            listed.created_at,
            Some(BIRTH_SECS),
            "a copy onto {} must keep the source's creation date (Unix {BIRTH_SECS}, 2015-03-01); it lists {:?}",
            dest.display(),
            listed.created_at
        );
    }
    if kept.contains(MetadataClasses::MODE) {
        assert_eq!(
            listed.permissions & 0o777,
            SOURCE_MODE,
            "a copy onto {} must keep the source's mode ({SOURCE_MODE:o}); it lists {:o}",
            dest.display(),
            listed.permissions
        );
    }
}

/// A folder [`Volume::apply_metadata`] was handed a creation date (and, when
/// `kept` holds [`MetadataClasses::MODE`], a mode) lists them, though writing its
/// contents happened first.
///
/// Creates `dir`, writes a file into it, then applies [`BIRTH_SECS`], mode
/// `0700`, and [`SOURCE_DATE_SECS`] in one call, and insists every class in
/// `kept` reports `Kept` and lists back, the modification date included. `dir`
/// must not exist yet.
pub async fn assert_apply_metadata_keeps(volume: &dyn Volume, dir: &Path, kept: MetadataClasses) {
    assert!(
        !volume.exists(dir).await,
        "fixture precondition: {} must not exist yet",
        dir.display()
    );
    volume
        .create_directory(dir)
        .await
        .unwrap_or_else(|e| panic!("{} must be creatable, got {e:?}", dir.display()));
    let content = b"a file that lands before its folder is dated\n".to_vec();
    let length = content.len() as u64;
    volume
        .write_from_stream(
            &dir.join("inside.txt"),
            WriteMode::CreateNew,
            StreamLength::Known(length),
            Box::new(CarryingSource::new(content)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap_or_else(|e| panic!("a file must land in {}, got {e:?}", dir.display()));

    let carried = CarriedMetadata {
        created: Some(UNIX_EPOCH + Duration::from_secs(BIRTH_SECS)),
        mode: kept.contains(MetadataClasses::MODE).then_some(FOLDER_MODE),
        ..CarriedMetadata::default()
    };
    let report = volume
        .apply_metadata(dir, &carried, Some(UNIX_EPOCH + Duration::from_secs(SOURCE_DATE_SECS)))
        .await;
    assert_eq!(report.modified, ClassOutcome::Kept, "the folder's date; got {report:?}");
    let listed = volume
        .get_metadata(dir)
        .await
        .unwrap_or_else(|e| panic!("{} must be stattable, got {e:?}", dir.display()));
    assert_eq!(listed.modified_at, Some(SOURCE_DATE_SECS), "the folder's date");
    if kept.contains(MetadataClasses::CREATED) {
        assert_eq!(
            report.created,
            ClassOutcome::Kept,
            "the folder's creation date; got {report:?}"
        );
        assert_eq!(
            listed.created_at,
            Some(BIRTH_SECS),
            "the folder {} must list the creation date it was given (Unix {BIRTH_SECS})",
            dir.display()
        );
    }
    if kept.contains(MetadataClasses::MODE) {
        assert_eq!(report.mode, ClassOutcome::Kept, "the folder's mode; got {report:?}");
        assert_eq!(
            listed.permissions & 0o777,
            FOLDER_MODE,
            "the folder {} must list the mode it was given",
            dir.display()
        );
    }
}

/// The read stream of `path` reports, for each class in `reported` a listing
/// can show, what `volume` lists for it: the creation date (to the second) and
/// the mode.
///
/// **Why this one is worth a shared assertion.** A destination can only keep
/// what the source stream reports. A source that lists a creation date a moment
/// ago but reports none on its stream makes every copy off it born at the copy.
pub async fn assert_read_stream_reports_the_listed_metadata(
    volume: &dyn Volume,
    path: &Path,
    reported: MetadataClasses,
) {
    let listed = volume
        .get_metadata(path)
        .await
        .unwrap_or_else(|e| panic!("fixture precondition: {} must be stattable, got {e:?}", path.display()));
    let stream = volume
        .open_read_stream(path)
        .await
        .unwrap_or_else(|e| panic!("{} must be readable, got {e:?}", path.display()));
    let carried = stream.metadata(MetadataClasses::ALL);
    if reported.contains(MetadataClasses::CREATED) {
        let listed_created = listed
            .created_at
            .unwrap_or_else(|| panic!("fixture precondition: {} must list a creation date", path.display()));
        let reported_created = carried
            .created
            .unwrap_or_else(|| {
                panic!(
                    "the read stream of {} reports no creation date, though the volume lists {listed_created}",
                    path.display()
                )
            })
            .duration_since(UNIX_EPOCH)
            .expect("a creation date after 1970")
            .as_secs();
        assert!(
            reported_created.abs_diff(listed_created) <= 1,
            "the read stream of {} must report the creation date the volume lists ({listed_created}); it reports {reported_created}",
            path.display()
        );
    }
    if reported.contains(MetadataClasses::MODE) {
        assert_eq!(
            carried.mode.map(|mode| mode & 0o777),
            Some(listed.permissions & 0o777),
            "the read stream of {} must report the mode the volume lists",
            path.display()
        );
    }
}

/// A source stream over bytes in hand that reports [`SOURCE_DATE_SECS`] as its
/// file's modification date, and [`BIRTH_SECS`] and mode `0600` as the rest.
struct CarryingSource {
    bytes: Option<Vec<u8>>,
    total: u64,
    read: u64,
}

impl CarryingSource {
    fn new(bytes: Vec<u8>) -> Self {
        let total = bytes.len() as u64;
        Self {
            bytes: Some(bytes),
            total,
            read: 0,
        }
    }
}

impl VolumeReadStream for CarryingSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let bytes = self.bytes.take()?;
            self.read += bytes.len() as u64;
            Some(Ok(bytes))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.total)
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    fn modified_at(&self) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(SOURCE_DATE_SECS))
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        CarriedMetadata {
            created: Some(UNIX_EPOCH + Duration::from_secs(BIRTH_SECS)),
            mode: Some(SOURCE_MODE),
            ..CarriedMetadata::default()
        }
        .restricted_to(wanted)
    }
}
