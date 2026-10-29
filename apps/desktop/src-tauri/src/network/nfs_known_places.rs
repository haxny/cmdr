//! The NFS exports this user has connected to, so the next time is one click.
//!
//! ❗ **One entry per PLACE, ❌ never per server.** An NFS server has many
//! places, one per export, and each is its own volume with its own id
//! (`cmdr_fs::volume::nfs_volume_id`), pin, switch, uid, and gid, so each is its
//! own entry; the servers listing groups them back under their server
//! (`saved_servers/nfs_places.rs`).
//!
//! ❗ **The NAME is the server's, ❌ never a place's**, the S3 store's rule
//! (`s3_known_places.rs`): a person names the server (host plus port), and an
//! export reads as its own path. So the store keeps one [`KnownNfsServer`] per
//! NAMED server beside the places; an unnamed server has no record and reads as
//! its host.
//!
//! ❗ **The uid and gid live HERE, in plain JSON, ❌ not the Keychain.** `AUTH_SYS`
//! has no secret: the uid is a preference like SMB's username, and a Keychain
//! prompt in front of a non-secret is friction. They're a SETTING of the place,
//! ❌ not its identity, so changing one keeps the id, the tabs, and the
//! favorites.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use cmdr_nfs::{NfsConnectionParams, NfsIdentity, NfsServer};
use serde::{Deserialize, Serialize};

use super::saved_server_fields;
use super::server_list_file;
use crate::state_recovery::StateFile;

use crate::ignore_poison::IgnorePoison;

/// One NFS export the user has connected to, and how to reach it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownNfsPlace {
    /// The server, as the person typed it: a name or an IP literal. ❗ Part of
    /// the identity, folded to lowercase there.
    pub host: String,
    /// The NFS port the address named, or `None` to ask the server's portmapper.
    /// ❗ Part of the identity as `port.unwrap_or(2049)`, ❌ never the port
    /// portmap answered.
    pub port: Option<u16>,
    /// The export, canonical (`cmdr_fs::volume::canonical_nfs_export`) once a
    /// connect resolved it. ❗ Part of the identity.
    pub export: String,
    /// The user id every request acts as. ❗ A setting, ❌ not the identity.
    pub uid: u32,
    /// The group id every request acts as. A setting, like `uid`.
    pub gid: u32,
    /// Where a pane lands when the place is opened: an export-relative folder
    /// (`/Photos`). `None` is the export's root.
    #[serde(default)]
    pub start_folder: Option<String>,
    /// Whether Cmdr may redial unattended when the connection drops. ❗ Defaults
    /// to on, the same as every other server store's.
    #[serde(default = "reconnects_automatically")]
    pub auto_reconnect: bool,
    /// Whether this place shows in the volume switcher. ❗ A `remember` never
    /// changes it on a replace (the WebDAV store's rule and reason).
    #[serde(default)]
    pub pinned: bool,
    /// When this place was last connected to, ISO 8601.
    #[serde(default)]
    pub last_connected_at: String,
}

impl KnownNfsPlace {
    /// The server this place is on.
    pub fn server(&self) -> NfsServer {
        NfsServer::new(&self.host, self.port)
    }

    /// The uid and gid this place acts as.
    pub fn identity(&self) -> NfsIdentity {
        NfsIdentity {
            uid: self.uid,
            gid: self.gid,
        }
    }

    /// The connection params this entry describes, the start folder and
    /// "reconnect automatically" included.
    pub fn params(&self) -> NfsConnectionParams {
        let mut params = NfsConnectionParams::new(self.server(), &self.export, self.identity());
        params.start_folder = self.start_folder.clone();
        params.auto_reconnect = self.auto_reconnect;
        params
    }

    /// The volume id this place registers under (`NfsConnectionParams::volume_id`).
    pub fn volume_id(&self) -> String {
        self.params().volume_id()
    }

    /// The id of the server this place belongs to ([`server_id`]).
    pub fn server_id(&self) -> String {
        server_id(&self.server())
    }

    /// The entry a connect's RESOLVED params describe: the real export, and the
    /// start folder the split left. `pinned` and `last_connected_at` are the
    /// caller's.
    pub fn from_params(params: &NfsConnectionParams, pinned: bool, last_connected_at: String) -> Self {
        Self {
            host: params.server.host.clone(),
            port: params.server.port,
            export: params.export().to_string(),
            uid: params.identity.uid,
            gid: params.identity.gid,
            start_folder: params.start_folder.clone(),
            auto_reconnect: params.auto_reconnect,
            pinned,
            last_connected_at,
        }
    }
}

/// A NAMED NFS server: the host and port its places share, and the name a
/// person gave it. ❗ An unnamed server has no record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownNfsServer {
    /// The host, as the person typed it.
    pub host: String,
    /// The NFS port the address named, if any.
    #[serde(default)]
    pub port: Option<u16>,
    /// The name a person gave the server, never blank.
    pub display_name: String,
}

impl KnownNfsServer {
    fn id(&self) -> String {
        server_id(&NfsServer::new(&self.host, self.port))
    }
}

/// The id a SERVER is listed under: the place id of its root export (`/`), which
/// folds the host and the port exactly the way every place id under it does,
/// whether or not that export is saved. The S3 store's account id, for the same
/// reason.
pub fn server_id(server: &NfsServer) -> String {
    cmdr_fs::volume::nfs_volume_id(&server.host, server.identity_port(), "/")
}

fn reconnects_automatically() -> bool {
    true
}

/// The whole store, as it sits on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownNfsPlacesStore {
    /// Every place, in the order they were first added.
    #[serde(default, deserialize_with = "crate::state_recovery::lenient_entries")]
    pub known_nfs_places: Vec<KnownNfsPlace>,
    /// Every named server, one record each.
    #[serde(default, deserialize_with = "crate::state_recovery::lenient_entries")]
    pub known_nfs_servers: Vec<KnownNfsServer>,
}

static KNOWN: OnceLock<Mutex<KnownNfsPlacesStore>> = OnceLock::new();
static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn known() -> &'static Mutex<KnownNfsPlacesStore> {
    KNOWN.get_or_init(|| Mutex::new(KnownNfsPlacesStore::default()))
}

/// Loads the store from the app's data dir. Call once at startup; a missing file is an
/// empty store, and one that doesn't read is set aside first (`server_list_file::read`).
pub fn load_known_nfs_places<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some((store, path)) =
        server_list_file::load::<_, KnownNfsPlacesStore>(app, "nfs_known_places.json", StateFile::NfsPlaces)
    else {
        return;
    };
    *known().lock_ignore_poison() = store;
    let _ = STORE_PATH.set(path);
}

fn save() {
    let Some(path) = STORE_PATH.get() else {
        // Before the load there's nowhere to write; a test binary lands here.
        return;
    };
    let store = known().lock_ignore_poison().clone();
    server_list_file::save(path, "the known NFS places", &store);
}

/// Every place the user has connected to.
pub fn all() -> Vec<KnownNfsPlace> {
    known().lock_ignore_poison().known_nfs_places.clone()
}

/// The entry whose place id is `volume_id`.
pub fn find(volume_id: &str) -> Option<KnownNfsPlace> {
    all().into_iter().find(|entry| entry.volume_id() == volume_id)
}

/// The name a person gave the server `place` is on, or empty when nobody did.
pub fn server_name(place: &KnownNfsPlace) -> String {
    let id = place.server_id();
    known()
        .lock_ignore_poison()
        .known_nfs_servers
        .iter()
        .find(|server| server.id() == id)
        .map(|server| server.display_name.clone())
        .unwrap_or_default()
}

/// What the UI calls the server `place` is on: its name, else its host
/// (`saved_server_fields::server_label`, with no account to put before it).
pub fn server_label(place: &KnownNfsPlace) -> String {
    server_label_named(&server_name(place), place)
}

fn server_label_named(name: &str, place: &KnownNfsPlace) -> String {
    saved_server_fields::server_label(name, "", &place.host)
}

/// What the UI calls a place: the export's path without its leading `/`
/// (`volume1/data`), and the root export by its server's label, since opening it
/// IS opening the whole server.
pub fn place_label(place: &KnownNfsPlace) -> String {
    place_label_named(&server_name(place), place)
}

/// [`place_label`] with the server's name given, for a connect that's naming it
/// in the same breath (its name isn't in the store until the dial lands).
pub fn place_label_named(name: &str, place: &KnownNfsPlace) -> String {
    let export = cmdr_fs::volume::canonical_nfs_export(&place.export);
    match export.strip_prefix('/') {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => server_label_named(name, place),
    }
}

/// Names `place`'s server with what an add or an edit TYPED, when it typed
/// anything. ❗ A blank Name leaves the server's name alone: adding a second
/// export under a named server without retyping its name must not unname it.
/// The newest name typed wins.
pub fn adopt_typed_name(place: &KnownNfsPlace, name: &str) {
    let name = name.trim();
    if !saved_server_fields::is_named(name) {
        return;
    }
    let id = place.server_id();
    {
        let mut store = known().lock_ignore_poison();
        store.known_nfs_servers.retain(|server| server.id() != id);
        store.known_nfs_servers.push(KnownNfsServer {
            host: place.host.clone(),
            port: place.port,
            display_name: name.to_string(),
        });
    }
    save();
}

/// Adds `place`, or replaces the entry with the same place id.
///
/// ❗ **`place.pinned` is honored only when the entry is NEW**, so a reconnect
/// can't re-pin a place the user unpinned (the WebDAV store's rule). An empty
/// `last_connected_at` is a save that connected nothing, so the stored time stays.
pub fn remember(mut place: KnownNfsPlace) {
    place.export = cmdr_fs::volume::canonical_nfs_export(&place.export);
    let volume_id = place.volume_id();
    {
        let mut store = known().lock_ignore_poison();
        match store
            .known_nfs_places
            .iter_mut()
            .find(|entry| entry.volume_id() == volume_id)
        {
            Some(existing) => {
                place.pinned = existing.pinned;
                if place.last_connected_at.is_empty() {
                    place.last_connected_at = std::mem::take(&mut existing.last_connected_at);
                }
                *existing = place;
            }
            None => store.known_nfs_places.push(place),
        }
    }
    save();
}

/// Applies `change` to the place `volume_id` names, in place under the lock,
/// answering whether there was one. ❗ Never a read-modify-`remember`: that
/// would write back every other field as it stood at the read.
fn update(volume_id: &str, change: impl FnOnce(&mut KnownNfsPlace)) -> bool {
    let moved = {
        let mut store = known().lock_ignore_poison();
        match store
            .known_nfs_places
            .iter_mut()
            .find(|entry| entry.volume_id() == volume_id)
        {
            Some(entry) => {
                change(entry);
                true
            }
            None => false,
        }
    };
    if moved {
        save();
    }
    moved
}

/// Moves the pin on one place.
pub fn set_pinned(volume_id: &str, pinned: bool) -> bool {
    update(volume_id, |entry| entry.pinned = pinned)
}

/// Moves "reconnect automatically" on one place. The live volume's copy is the
/// wiring's job (`nfs_volume_wiring::apply_auto_reconnect`).
pub fn set_auto_reconnect(volume_id: &str, on: bool) -> bool {
    update(volume_id, |entry| entry.auto_reconnect = on)
}

/// Drops one place, answering whether it was there. The server's name goes
/// with its LAST place, so adding the server again starts unnamed.
pub fn forget(volume_id: &str) -> bool {
    let removed = {
        let mut store = known().lock_ignore_poison();
        let before = store.known_nfs_places.len();
        store.known_nfs_places.retain(|entry| entry.volume_id() != volume_id);
        let still_saved: std::collections::HashSet<String> =
            store.known_nfs_places.iter().map(KnownNfsPlace::server_id).collect();
        store
            .known_nfs_servers
            .retain(|server| still_saved.contains(&server.id()));
        store.known_nfs_places.len() != before
    };
    if removed {
        save();
    }
    removed
}

#[cfg(test)]
#[path = "nfs_known_places_test.rs"]
mod nfs_known_places_test;
