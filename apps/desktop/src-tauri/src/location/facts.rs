//! The volume facts the resolver reads, gathered from in-memory sources only.
//!
//! The volume registry's live set, the saved-server and saved-share stores, the
//! cached device list, and the index's own state: ❌ never a `statfs`, a mount
//! table read, or the full volume listing (up to 2 s on a wedged mount). A tool
//! call resolves a location before it does anything else, so this has to answer
//! in microseconds whatever the network is doing.
//!
//! It also owns the earlier-roots record ([`RootHistory`]): every root a volume
//! held this session, seeded with each saved share's last mount path, so a
//! location a model got before a remount still resolves after it.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use cmdr_fs::volume::{BackendKind, ConnectionState, MountClass};

use super::resolve::{IndexSources, RootSources, VolumeFact, VolumeFacts, volume_root};
use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;
use crate::index_host::index;
use crate::mcp::resources::volumes::VolumeKind;

/// Every root each volume answered at this session, by volume id, oldest first.
///
/// In memory only: the saved share's last mount path is the one root that
/// outlives a session, and it's re-seeded at every start.
#[derive(Default)]
pub(crate) struct RootHistory {
    roots: Mutex<HashMap<String, Vec<String>>>,
}

impl RootHistory {
    /// Note that `id` answered at `root`. Recording a known root again is a no-op.
    pub(crate) fn record(&self, id: &str, root: &str) {
        let mut roots = self.roots.lock_ignore_poison();
        let held = roots.entry(id.to_string()).or_default();
        if !held.iter().any(|known| known == root) {
            held.push(root.to_string());
        }
    }

    /// The roots `id` held before `current`, for [`VolumeFact::earlier_roots`].
    pub(crate) fn earlier_roots(&self, id: &str, current: &str) -> Vec<String> {
        self.roots
            .lock_ignore_poison()
            .get(id)
            .map(|held| held.iter().filter(|root| *root != current).cloned().collect())
            .unwrap_or_default()
    }
}

static ROOT_HISTORY: LazyLock<RootHistory> = LazyLock::new(RootHistory::default);

/// Start the earlier-roots record: seed it with every saved share's last mount
/// path, record each registered volume's root, and listen for every volume that
/// arrives from here on.
///
/// Call once at startup, after the saved-share store loads (its last mount paths
/// are the seed) and before a remount could move one. A root move that isn't an
/// arrival (a renamed drive) is caught by [`VolumeFacts::now`], which records
/// every live root it reads.
pub(crate) fn start_root_history() {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    for share in crate::network::known_shares::saved_shares() {
        if let (Some(id), Some(mount_path)) = (share.volume_id, share.mount_path) {
            ROOT_HISTORY.record(&id, &mount_path);
        }
    }
    let manager = get_volume_manager();
    for (id, volume) in manager.list_volumes_with_handles() {
        ROOT_HISTORY.record(&id, &volume.root().to_string_lossy());
    }
    // Runs INSIDE the registration: one lookup and one map insert, then return.
    manager.on_volume_arrival(|id| {
        if let Some(volume) = get_volume_manager().get(id) {
            ROOT_HISTORY.record(id, &volume.root().to_string_lossy());
        }
    });
}

impl VolumeFacts {
    /// Every volume the app knows right now: registered ones (live), then the
    /// saved servers, saved SMB shares, and listed devices that aren't registered.
    pub(crate) async fn now() -> Self {
        let manager = get_volume_manager();
        let mut volumes: Vec<VolumeFact> = Vec::new();

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let saved_shares = crate::network::known_shares::saved_shares();

        for (id, volume) in manager.list_volumes_with_handles() {
            // An archive, a `.git` portal, or a snapshot tree is a view INTO a
            // volume, not a place a location lives on.
            if volume.routes_over_a_parent() {
                continue;
            }
            let Some(kind) = kind_of_backend(volume.backend_kind(), volume.mount_class()) else {
                continue;
            };
            let live_root = volume.root().to_string_lossy().into_owned();
            ROOT_HISTORY.record(&id, &live_root);
            let Some(root) = volume_root(RootSources {
                live: Some(&live_root),
                ..RootSources::default()
            }) else {
                continue;
            };
            // A share reads as the switcher spells it ("naspi on naspolya") when
            // a saved row names it; the mount's own name is only the share's.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            let name = saved_shares
                .iter()
                .find(|share| share.volume_id.as_deref() == Some(id.as_str()))
                .map(|share| format!("{} on {}", share.share_name, share.server_name))
                .unwrap_or_else(|| volume.name().to_string());
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let name = volume.name().to_string();
            volumes.push(fact(id, name, kind, root, true, volume.connection_state()));
        }

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            for place in crate::server_volumes::server_places() {
                if known(&volumes, &place.id) {
                    continue;
                }
                let Some(kind) = kind_of_fs_type(place.fs_type) else {
                    continue;
                };
                let Some(root) = volume_root(RootSources {
                    saved: Some(&place.app_root),
                    ..RootSources::default()
                }) else {
                    continue;
                };
                volumes.push(fact(place.id, place.name, kind, root, false, Some(place.state)));
            }
            for share in saved_shares {
                let (Some(id), Some(mount_path)) = (share.volume_id, share.mount_path) else {
                    continue;
                };
                if known(&volumes, &id) {
                    continue;
                }
                // The index's recorded root has no reader for a stopped volume yet,
                // so a saved share's root is its last mount path.
                let Some(root) = volume_root(RootSources {
                    saved: Some(&mount_path),
                    ..RootSources::default()
                }) else {
                    continue;
                };
                let name = format!("{} on {}", share.share_name, share.server_name);
                volumes.push(fact(
                    id,
                    name,
                    VolumeKind::Smb,
                    root,
                    false,
                    Some(ConnectionState::Saved),
                ));
            }
        }

        // A device listed but not dialed yet (a phone waiting for its "Allow USB
        // debugging?" tap) still has an id a model may name.
        let mut devices = Vec::new();
        crate::device_volumes::append_device_volumes(&mut devices).await;
        for device in devices {
            if known(&volumes, &device.id) {
                continue;
            }
            let Some(kind) = device.fs_type.as_deref().and_then(kind_of_fs_type) else {
                continue;
            };
            let Some(root) = volume_root(RootSources {
                saved: Some(&device.path),
                ..RootSources::default()
            }) else {
                continue;
            };
            volumes.push(fact(device.id, device.name, kind, root, false, device.connection_state));
        }

        Self {
            volumes,
            mount_areas: MOUNT_AREAS.iter().map(|area| (*area).to_string()).collect(),
        }
    }
}

/// Where the OS mounts volumes. A Linux `/mnt` is left out: it's a plain folder
/// as often as a mount point.
#[cfg(target_os = "macos")]
const MOUNT_AREAS: &[&str] = &["/Volumes"];
#[cfg(target_os = "linux")]
const MOUNT_AREAS: &[&str] = &["/media", "/run/media"];
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const MOUNT_AREAS: &[&str] = &[];

fn known(volumes: &[VolumeFact], id: &str) -> bool {
    volumes.iter().any(|v| v.id == id)
}

fn fact(
    id: String,
    name: String,
    kind: VolumeKind,
    root: String,
    live: bool,
    connection_state: Option<ConnectionState>,
) -> VolumeFact {
    // A live volume's index is whichever one the index routes its root to (a cloud
    // drive's is `root`'s). A volume that isn't live has only its own: routing a
    // dead mount point would answer `root`, the very mix-up the resolver exists
    // to stop.
    let index_id = if live {
        index().volume_id_for_path(&root)
    } else {
        id.clone()
    };
    let index_sources = IndexSources {
        live: index().volume_status(&index_id).enabled,
        stored: crate::search::volumes::has_searchable_index(&index_id),
        volume_id: index_id,
    };
    VolumeFact {
        earlier_roots: ROOT_HISTORY.earlier_roots(&id, &root),
        id,
        name,
        kind,
        root,
        live,
        connection_state,
        index: index_sources,
    }
}

/// A registered volume's `kind`, from its backend and the mount it sits on. `None`
/// for a backend that's a view into another volume.
///
/// An OS-mounted share is served by `LocalPosixVolume`, so the backend alone
/// would call it local: the mount class is what says "SMB".
fn kind_of_backend(backend: BackendKind, mount: Option<MountClass>) -> Option<VolumeKind> {
    Some(match backend {
        BackendKind::Local => match mount {
            Some(MountClass::SmbShare) => VolumeKind::Smb,
            Some(MountClass::OtherNetwork) => VolumeKind::Network,
            Some(MountClass::LocalDisk) | None => VolumeKind::Local,
        },
        BackendKind::Smb => VolumeKind::Smb,
        BackendKind::Sftp => VolumeKind::Sftp,
        BackendKind::Webdav => VolumeKind::Webdav,
        BackendKind::S3 => VolumeKind::S3,
        BackendKind::Nfs => VolumeKind::Network,
        BackendKind::Mtp => VolumeKind::Mtp,
        BackendKind::Adb => VolumeKind::Adb,
        BackendKind::Archive | BackendKind::GitPortal | BackendKind::Restic => return None,
    })
}

/// A saved place's or listed device's `kind`, from the `fs_type` its row carries.
fn kind_of_fs_type(fs_type: &str) -> Option<VolumeKind> {
    Some(match fs_type {
        "sftp" => VolumeKind::Sftp,
        "webdav" => VolumeKind::Webdav,
        "s3" => VolumeKind::S3,
        "nfs" => VolumeKind::Network,
        "mtp" => VolumeKind::Mtp,
        "adb" => VolumeKind::Adb,
        _ => return None,
    })
}
