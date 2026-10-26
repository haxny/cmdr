//! Volume and location discovery for Linux.
//!
//! Provides a sidebar location picker with:
//! - Favorites (Home, Desktop, Documents, Downloads)
//! - Main volume (root `/`)
//! - Mounted volumes (real filesystems from /proc/mounts)
//! - Cloud drives (Dropbox, Google Drive, Nextcloud, OneDrive)
//! - Network mounts (GVFS SMB shares under `/run/user/<uid>/gvfs/`)
//! - Removable media under /run/media/ or /media/
//!
//! The discovery primitives are split across themed submodules, mirroring macOS
//! `volumes/`; this module holds the shared model types, consts, and the
//! orchestrators that assemble them, and re-exports every submodule item so
//! `crate::volumes_linux::X` paths stay stable.

pub mod watcher;

mod cloud;
mod fs_type;
mod ids;
mod mounts;
mod smb;

#[cfg(test)]
mod test_support;

pub use fs_type::{get_volume_space, supports_trash_for_mount};
pub use mounts::get_mounted_volumes;
pub use smb::{SmbMountInfo, enrich_from_volume_registry, get_smb_mount_info};

pub(crate) use fs_type::get_mount_point;
pub(crate) use ids::volume_id_for_mount;
pub(crate) use mounts::{mount_roots, registrable_mount_roots};
pub(crate) use smb::{parse_gvfs_smb_dirname, smb_mounts};

#[allow(
    unused_imports,
    reason = "API parity with macOS volumes module; used once SMB enrichment lands on Linux"
)]
pub use crate::file_system::volume::ConnectionState;

use cmdr_fs::volume::published_locations::{PublishedLocation, dedupe_locations};
use cmdr_local::linux_mounts::{self, MountEntry};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Category of a location item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LocationCategory {
    Favorite,
    MainVolume,
    AttachedVolume,
    CloudDrive,
    Network,
    MobileDevice,
}

/// Information about a location (volume, folder, or cloud drive).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: LocationCategory,
    pub icon: Option<String>,
    pub is_ejectable: bool,
    pub fs_type: Option<String>,
    pub supports_trash: bool,
    /// Whether the MOUNT behind this location refuses writes right now (for example, an MTP
    /// device reporting locked storage). Orthogonal to `capabilities.backend_can_write`, which
    /// answers for the BACKEND. Mirrors the macOS field; see `volumes/mod.rs`.
    pub mount_is_read_only: bool,
    /// Whether this volume is a mounted disk image (`.dmg`). Always `false` on Linux;
    /// mirrors the macOS shape so the shared `LocationInfo`/`VolumeInfo` type stays identical.
    pub is_disk_image: bool,
    /// Twin of the macOS field: whether a cloud provider's own filesystem serves this mount, so
    /// every entry read costs a round trip to that provider's daemon. Groups the row under CLOUD
    /// and suppresses the index affordances. Always `false` here today: the Linux cloud clients
    /// Cmdr recognizes sync into ordinary directories rather than mounting their own filesystem.
    pub is_cloud_mount: bool,
    /// SMB connection state indicator. Always `None` on Linux (no smb2 session tracking yet).
    pub connection_state: Option<ConnectionState>,
    /// Twin of the macOS field: whether the DEVICE behind this row is reachable.
    /// Whether this place belongs in the volume SWITCHER: the user's own cap on
    /// how many saved things crowd their disks. `Some` only on a server place
    /// (`server_volumes.rs`), which is the only row the cap applies to; `None`
    /// on a local disk, a favorite, and a mounted SMB share, all of which show
    /// unconditionally.
    ///
    /// ❗ The listing publishes EVERY saved place regardless, because a volume id
    /// with no row is one the app denies exists (a hub Enter and a restored tab
    /// both land on an id). Hiding is `navigation/volume-grouping.ts`'s job, and
    /// this field is what it reads.
    pub pinned: Option<bool>,
    /// Twin of the macOS field: where opening a server place lands, as an app
    /// path, when that isn't `path`. `None` lands at `path`.
    pub landing_path: Option<String>,
    pub device_readiness: Option<cmdr_fs::volume::DeviceReadiness>,
    /// Negotiated USB link speed. Set only for MTP/mobile volumes; everything
    /// else carries `None`. Frontend maps to a label like "USB 3.2 Gen 1".
    pub usb_speed: Option<crate::usb_speed::UsbSpeed>,
    /// What the backend registered for this volume can do (writable? can it be a
    /// copy source?), straight from `Volume::capabilities()`, so the frontend
    /// never re-derives capability from an id, an `fsType`, or a category.
    /// `None` when no backend is registered for this id (a favorite, or a volume
    /// discovery found before registration): the frontend falls back to its
    /// per-kind defaults. Filled by `enrich_from_volume_registry`, never by a
    /// discovery constructor.
    pub capabilities: Option<cmdr_fs::volume::VolumeCapabilities>,
    /// Single-letter menu shortcut, present only on favorite rows.
    pub favorite_shortcut: Option<String>,
    /// Twin of the macOS field: present only on a favorite row (`favorites/target.rs`).
    pub favorite_target: Option<crate::favorites::target::FavoriteTarget>,
    /// What a tab at this volume's root is called, when the mount directory's
    /// name isn't it: an SMB share's own name. The macOS twin says why.
    pub root_label: Option<String>,
    /// The account a mounted SMB share is signed in as. The macOS twin says why.
    pub mount_account: Option<String>,
}

/// Lets discovery collapse a filesystem mounted at several paths down to one
/// published location (`cmdr_fs::volume::canonical_root`). The macOS twin
/// implements the same trait on its own `LocationInfo`, which is what keeps the
/// rule shared without merging the two types.
impl cmdr_fs::volume::canonical_root::MountRootCandidate for LocationInfo {
    fn volume_id(&self) -> &str {
        &self.id
    }

    fn mount_root(&self) -> &str {
        &self.path
    }
}

impl PublishedLocation for LocationInfo {
    fn location_id(&self) -> &str {
        &self.id
    }

    fn location_path(&self) -> &str {
        &self.path
    }

    fn is_favorite(&self) -> bool {
        self.category == LocationCategory::Favorite
    }
}

/// The volume-shaped name for the same struct; see the macOS twin in `volumes/mod.rs`.
pub use LocationInfo as VolumeInfo;

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Get all locations organized by category, deduplicated.
///
/// Gathers favorites, the main volume, mounted volumes (real filesystems,
/// excluding root and virtual), cloud drives, and GVFS SMB shares in that
/// order, then dedupes through `cmdr_fs::volume::published_locations` (shared
/// with macOS), whose header says which row wins a clash. The ID half of that
/// rule catches one filesystem reachable through two categories (a CIFS mount
/// that's also GVFS-mounted); `get_mounted_volumes` already collapses double
/// mounts within its own category.
pub fn list_locations() -> Vec<LocationInfo> {
    // An unreadable table lists no attached volumes; favorites, root, cloud
    // drives, and GVFS shares don't come from it, so they still show.
    let mounts = linux_mounts::parse_proc_mounts().unwrap_or_default();
    let locations = get_favorites(&mounts)
        .into_iter()
        .chain(get_main_volume(&mounts))
        .chain(get_mounted_volumes(&mounts))
        .chain(cloud::get_cloud_drives(&mounts))
        .chain(smb::get_network_mounts());
    dedupe_locations(locations)
}

/// Get the user's favorites from the editable store (`favorites.json`), one row each.
///
/// ❗ Every stored favorite publishes a row, a missing one included; the reach pass
/// (`favorites/reach.rs`) words it. Seeds the platform defaults on first launch (file absent); see
/// `favorites/CLAUDE.md`.
fn get_favorites(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    crate::favorites::store::list()
        .into_iter()
        .map(|favorite| favorite_location(favorite, mounts, |path| path.exists()))
        .collect()
}

/// One favorite's discovery row, the twin of macOS `volumes::favorite_location`. ❗ No syscall on a
/// network path, ever: the containing mount's type comes from the parsed table, and only a folder
/// on a local disk is stat'd (`exists`, injected so a test can prove which ones aren't). Linux has
/// no TCC, so there's no FDA-pending skip.
fn favorite_location(
    favorite: crate::favorites::store::Favorite,
    mounts: &[MountEntry],
    exists: impl FnOnce(&Path) -> bool,
) -> LocationInfo {
    use crate::favorites::target::{FavoriteTarget, Probe, on_disk};

    let path = Path::new(&favorite.path);
    // ❗ Only an OS path has a containing mount: the table lookup matches `/` for anything.
    let mount = path
        .is_absolute()
        .then(|| linux_mounts::mount_entry_for_path_from_entries(path, mounts))
        .flatten();
    let fs_type = mount.map(|mount| mount.fstype.clone());
    let probe = match fs_type.as_deref().is_some_and(linux_mounts::is_network_fs_type) {
        true => Probe::NetworkMount,
        false => Probe::Stat,
    };
    let on_disk = on_disk(&favorite.path, probe, exists);
    LocationInfo {
        id: format!("fav-{}", favorite.id),
        name: favorite.name,
        // A scheme path has no mount, so no OS trash either.
        supports_trash: mount.is_some_and(|mount| supports_trash_for_mount(Some(&mount.device), Some(&mount.fstype))),
        path: favorite.path,
        category: LocationCategory::Favorite,
        icon: None,
        is_ejectable: false,
        fs_type,
        mount_is_read_only: false,
        is_disk_image: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: favorite.shortcut,
        favorite_target: Some(FavoriteTarget::discovered(favorite.volume, on_disk)),
        root_label: None,
        mount_account: None,
    }
}

/// Get the root filesystem as the main volume.
fn get_main_volume(mounts: &[MountEntry]) -> Option<LocationInfo> {
    let mount = linux_mounts::mount_entry_for_path_from_entries(Path::new("/"), mounts);
    let fs_type = mount.map(|mount| mount.fstype.clone());
    let supports_trash = mount.is_some_and(|mount| supports_trash_for_mount(Some(&mount.device), Some(&mount.fstype)));
    Some(LocationInfo {
        id: DEFAULT_VOLUME_ID.to_string(),
        name: "Root".to_string(),
        path: "/".to_string(),
        category: LocationCategory::MainVolume,
        icon: None,
        is_ejectable: false,
        fs_type,
        supports_trash,
        mount_is_read_only: false,
        is_disk_image: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    })
}

/// Build a `VolumeInfo` for the volume containing `path` using only
/// mount table data. Does NOT call `list_locations()`.
pub fn resolve_path_volume_fast(path: &str) -> Option<VolumeInfo> {
    // A GVFS share never reaches the mount table (one FUSE mount serves them all), so the
    // walk below would answer that FUSE root: the share's own row, as discovery lists it.
    if let Some(root) = smb::gvfs_share_root(path) {
        let dirname = Path::new(root).file_name()?.to_str()?;
        let gvfs = parse_gvfs_smb_dirname(dirname)?;
        return Some(smb::gvfs_share_location(root.to_string(), gvfs.share));
    }
    let (mount_point, fs_type, source) = get_mount_point(path)?;

    let name = mounts::mount_display_name(&mount_point);
    let supports_trash = supports_trash_for_mount(Some(&source), Some(&fs_type));
    let category = if mount_point == "/" {
        LocationCategory::MainVolume
    } else {
        LocationCategory::AttachedVolume
    };

    Some(VolumeInfo {
        id: volume_id_for_mount(&mount_point),
        name,
        path: mount_point,
        category,
        icon: None,
        is_ejectable: false,
        fs_type: Some(fs_type),
        supports_trash,
        mount_is_read_only: false,
        is_disk_image: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    })
}

/// The twin of macOS `volumes::network_mount_under`, named as `resolve_path_volume_fast` names it.
pub(crate) fn network_mount_under(path: &Path) -> Option<crate::volume_listing::NetworkMount> {
    network_mount_in(&linux_mounts::parse_proc_mounts()?, path)
}

/// [`network_mount_under`] over a table already read.
///
/// ❗ A GVFS SMB share answers its own folder, the way `resolve_path_volume_fast` does, ❌ never the
/// FUSE root the table names: one FUSE mount serves every share, and its root is a volume no row
/// lists, so a favorite saved on it reads "unplugged" (`DETAILS.md` § "A path inside a GVFS share
/// resolves to the share").
fn network_mount_in(mounts: &[MountEntry], path: &Path) -> Option<crate::volume_listing::NetworkMount> {
    if let Some(root) = path.to_str().and_then(smb::gvfs_share_root) {
        let dirname = Path::new(root).file_name()?.to_str()?;
        return Some(crate::volume_listing::NetworkMount {
            mount_point: root.to_string(),
            name: parse_gvfs_smb_dirname(dirname)?.share,
        });
    }
    let entry = linux_mounts::mount_entry_for_path_from_entries(path, mounts)?;
    if cmdr_fs::filesystem_kind::mount_is_local_disk(Some(&entry.device), Some(&entry.fstype)) {
        return None;
    }
    Some(crate::volume_listing::NetworkMount {
        mount_point: entry.mountpoint.clone(),
        name: mounts::mount_display_name(&entry.mountpoint),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::parse_test_mounts;

    fn proc_mount(device: &str, mountpoint: &str, fstype: &str) -> MountEntry {
        MountEntry {
            device: device.to_string(),
            mountpoint: mountpoint.to_string(),
            fstype: fstype.to_string(),
            options: String::new(),
        }
    }

    #[test]
    fn network_mount_in_names_the_share_and_skips_local_disks() {
        let mounts = vec![
            proc_mount("/dev/sda1", "/", "ext4"),
            proc_mount("//nas/media", "/mnt/media", "cifs"),
            proc_mount("/dev/sdb1", "/mnt/media-1", "ext4"),
        ];
        let network = |path: &str| network_mount_in(&mounts, Path::new(path));
        assert_eq!(
            network("/mnt/media/films"),
            Some(crate::volume_listing::NetworkMount {
                mount_point: "/mnt/media".to_string(),
                name: "media".to_string(),
            })
        );
        assert_eq!(network("/mnt/media-1/x"), None);
        assert_eq!(network("/home/me"), None);
    }

    /// ❗ One FUSE mount serves every GVFS share, so the table's own answer for a folder on one is
    /// that FUSE root. The add gate would then favorite the folder on the root's volume, which no
    /// row lists ("unplugged"), rather than on the share's (`favorites-offline-smb.spec.ts`).
    #[test]
    fn network_mount_in_names_a_gvfs_share_rather_than_the_fuse_root() {
        let mounts = vec![
            proc_mount("/dev/sda1", "/", "ext4"),
            proc_mount("gvfsd-fuse", "/run/user/1000/gvfs", "fuse.gvfsd-fuse"),
        ];
        let share = "/run/user/1000/gvfs/smb-share:server=nas,share=public";
        assert_eq!(
            network_mount_in(&mounts, Path::new(&format!("{share}/docs/2026"))),
            Some(crate::volume_listing::NetworkMount {
                mount_point: share.to_string(),
                name: "public".to_string(),
            })
        );
        // Anything else GVFS serves has no row of its own, and the resolver answers the FUSE root.
        assert_eq!(
            network_mount_in(&mounts, Path::new("/run/user/1000/gvfs/sftp:host=nas/home")),
            Some(crate::volume_listing::NetworkMount {
                mount_point: "/run/user/1000/gvfs".to_string(),
                name: mounts::mount_display_name("/run/user/1000/gvfs"),
            })
        );
    }

    #[test]
    fn test_get_main_volume() {
        let mounts = parse_test_mounts();
        let main = get_main_volume(&mounts);
        assert!(main.is_some());
        let main = main.unwrap();
        assert_eq!(main.id, "root");
        assert_eq!(main.path, "/");
        assert_eq!(main.category, LocationCategory::MainVolume);
        assert_eq!(main.fs_type.as_deref(), Some("ext4"));
    }

    // -- favorites discovery --

    use crate::favorites::store::Favorite;
    use crate::favorites::target::{FavoriteReach, OnDisk};

    fn favorite(path: &str) -> Favorite {
        Favorite {
            id: "f1".to_string(),
            path: path.to_string(),
            name: "docs".to_string(),
            shortcut: None,
            volume: None,
        }
    }

    fn with_a_share() -> Vec<MountEntry> {
        linux_mounts::parse_proc_mounts_from_content(
            "/dev/sda1 / ext4 rw,relatime 0 0\n//nas/naspi /mnt/naspi cifs rw,relatime 0 0\n",
        )
    }

    fn never_asked(path: &Path) -> bool {
        panic!("discovery must not stat {path:?}")
    }

    /// ❗ The hung-mount guard: a favorite on a wedged CIFS share gets no `exists()`.
    #[test]
    fn a_favorite_on_a_network_mount_is_listed_without_touching_it() {
        let row = favorite_location(favorite("/mnt/naspi/docs"), &with_a_share(), never_asked);
        assert_eq!(row.fs_type.as_deref(), Some("cifs"));
        let target = row.favorite_target.expect("a favorite row carries its target");
        assert_eq!(target.discovered.on_disk, OnDisk::Unchecked);
    }

    /// ❗ No favorite is filtered out any more: a missing folder still publishes its row.
    #[test]
    fn a_missing_local_favorite_still_publishes_a_row() {
        let row = favorite_location(favorite("/home/nobody/gone"), &with_a_share(), |_| false);
        assert_eq!(row.id, "fav-f1");
        let target = row.favorite_target.expect("a favorite row carries its target");
        assert_eq!(target.discovered.on_disk, OnDisk::No);
        assert_eq!(target.reach, FavoriteReach::NotFound);
    }

    #[test]
    fn a_scheme_path_favorite_has_no_mount() {
        let row = favorite_location(favorite("sftp://ada@nas:22/srv"), &with_a_share(), never_asked);
        assert_eq!(row.fs_type, None);
        assert!(!row.supports_trash);
    }
}
