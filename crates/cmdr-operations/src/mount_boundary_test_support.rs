//! A folder with a real drive mounted inside it, for the disk-image lane's
//! mount-wall pins (`delete::real_image`, `transfer::real_image`).
//!
//! No hook can stand in for a mount: the walkers' first test is a device number
//! that differs from the parent's, and the Overwrite check reads the OS mount
//! table, so only a real mount produces either.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::testing::TestDir;
use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec};

use crate::types::WriteOperationError;

/// `folder/` holds `mine.txt` and `mnt/`, where an APFS image is mounted with
/// `precious.txt` on it.
///
/// ❗ Field order is drop order: the image detaches before the folder holding
/// its mount point goes.
pub(crate) struct NestedMount {
    _image: DiskImage,
    pub(crate) dir: TestDir,
}

impl NestedMount {
    pub(crate) fn mount() -> Self {
        Self::mount_in(&DiskImageSession::acquire())
    }

    /// [`Self::mount`] under a session the caller holds, for a test that also
    /// attaches a drive of its own (a session per thread is the harness's rule).
    pub(crate) fn mount_in(session: &Arc<DiskImageSession>) -> Self {
        let dir = TestDir::new("nested_mount");
        std::fs::create_dir_all(dir.join("folder/mnt")).expect("the mount point");
        std::fs::write(dir.join("folder/mine.txt"), b"MINE").expect("a file beside the mount");
        let mut image = DiskImage::attach(session, ImageSpec::Apfs).expect("attach the image");
        let mounted = image
            .remount_volume_at(0, &dir.join("folder/mnt"))
            .expect("mount it inside the folder")
            .mount_point
            .clone();
        std::fs::write(mounted.join("precious.txt"), b"PRECIOUS").expect("a file on the image");
        assert_eq!(
            std::fs::read(dir.join("folder/mnt/precious.txt")).ok().as_deref(),
            Some(&b"PRECIOUS"[..]),
            "precondition: the image is reachable through the folder"
        );
        Self { _image: image, dir }
    }

    pub(crate) fn folder(&self) -> PathBuf {
        self.dir.join("folder")
    }

    /// Both files where they were: the drive's, and the one beside the mount,
    /// since the refusal comes before anything is removed or set aside.
    pub(crate) fn assert_untouched(&self) {
        assert_eq!(
            std::fs::read(self.dir.join("folder/mnt/precious.txt")).ok().as_deref(),
            Some(&b"PRECIOUS"[..]),
            "❗ the mounted drive's file must survive"
        );
        assert_eq!(
            std::fs::read(self.dir.join("folder/mine.txt")).ok().as_deref(),
            Some(&b"MINE"[..]),
            "the refusal comes before anything is removed"
        );
    }
}

/// `result` is the typed refusal, naming the mount.
pub(crate) fn assert_refused_at_the_mount<T: std::fmt::Debug>(result: Result<T, WriteOperationError>) {
    match result {
        Err(WriteOperationError::MountedVolumeInside { mount_point, .. }) => assert!(
            Path::new(&mount_point).ends_with("folder/mnt"),
            "names the mount: {mount_point}"
        ),
        other => panic!("expected the mounted-volume refusal, got {other:?}"),
    }
}
