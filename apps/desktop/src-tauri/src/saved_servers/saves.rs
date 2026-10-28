//! The per-protocol save behind `update_saved_server`: an add without a dial, an
//! edit in place, or an edit that moves a server to a new address.

use super::nfs_places::nfs_place;
use super::s3_accounts::s3_place;
use super::wire::ServerTarget;
use super::{SavedEntry, saved_by_id};
use crate::network::nfs_known_places::KnownNfsPlace;
use crate::network::nfs_volume_wiring;
use crate::network::s3_known_places::KnownS3Place;
use crate::network::s3_volume_wiring;
use crate::network::saved_server_fields::SavedServerOutcome;
use crate::network::sftp_known_servers::KnownSftpServer;
use crate::network::sftp_volume_wiring;
use crate::network::webdav_known_servers::KnownWebdavServer;
use crate::network::webdav_volume_wiring;

/// Saves `server` with no saved entry behind the request: an "Add anyway".
///
/// ❗ Calls each wiring's `save_without_connecting` directly: there is no
/// per-protocol IPC command behind it any more (nothing but its own test ever
/// called `update_known_sftp_server` / `update_known_webdav_server`), so this
/// facade builds the known-servers entry itself instead of forwarding.
pub(crate) async fn save_target(server: ServerTarget) -> SavedServerOutcome {
    let Some(entry) = entry_of(server) else {
        return SavedServerOutcome::Unreachable;
    };
    save_entry(entry).await
}

async fn save_entry(entry: Entry) -> SavedServerOutcome {
    match entry {
        Entry::Sftp(entry) => sftp_volume_wiring::save_without_connecting(entry).await,
        Entry::Webdav(entry) => webdav_volume_wiring::save_without_connecting(entry).await,
        Entry::S3 { place, account_name } => s3_volume_wiring::save_without_connecting(place, &account_name).await,
        Entry::Nfs { place, server_name } => nfs_volume_wiring::save_without_connecting(place, &server_name).await,
    }
}

/// Saves an edit to the saved place `id` names: in place when the address is the
/// same, else as a move to the new address (`server_move.rs`).
///
/// ❗ A place nobody saved any more (a Forget in another pane) answers
/// `Unreachable`, the save nobody could confirm, ❌ never a quiet re-save. One
/// naming another protocol or account answers `AccountChanged`.
pub(crate) async fn save_edit(id: &str, server: ServerTarget) -> SavedServerOutcome {
    let Some(saved) = saved_by_id(id) else {
        return SavedServerOutcome::Unreachable;
    };
    let Some(edit) = entry_of(server) else {
        return SavedServerOutcome::Unreachable;
    };
    match (saved, edit) {
        (SavedEntry::Sftp(saved), Entry::Sftp(edit)) => crate::server_move::edit_sftp(saved, edit).await,
        (SavedEntry::Webdav(saved), Entry::Webdav(edit)) => crate::server_move::edit_webdav(saved, edit).await,
        // A place's endpoint is its ACCOUNT's, which only the account's own edit
        // moves (`update_saved_s3_account`), so a place's edit keeps its id.
        (SavedEntry::S3(_), entry @ Entry::S3 { .. }) if entry.s3_place_id().as_deref() == Some(id) => {
            save_entry(entry).await
        }
        // ❗ The host, port, and export are the place itself; the uid, gid, name,
        // start folder, and switch are what an edit moves. A changed address is
        // another place, so it's refused rather than saved beside this one.
        (SavedEntry::Nfs(_), entry @ Entry::Nfs { .. }) if entry.nfs_place_id().as_deref() == Some(id) => {
            save_entry(entry).await
        }
        _ => SavedServerOutcome::AccountChanged,
    }
}

/// What an edit of one saved place sets, each as its NEW value. Nothing else is
/// here on purpose: the address, the account, and every switch (key file, SSH
/// agent, "Reconnect automatically") stay as stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PlaceEdit {
    /// The name the servers list shows; `None` keeps the stored one. For S3 and
    /// NFS it is the account's or server's name, as their targets carry it.
    pub display_name: Option<String>,
    /// SFTP and WebDAV's root; `None` keeps the stored one.
    pub remote_root: Option<String>,
    /// Where a pane opens; `None` opens at the root.
    pub start_folder: Option<String>,
    /// NFS's identity; `None` keeps the stored one.
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

/// Saves an edit of the saved place `id` that changes only `edit`'s fields: the
/// stored entry with those set, through [`save_edit`], so nothing the edit
/// doesn't name can move. The edit an approved Ask Cmdr suggestion saves.
///
/// ❗ An SMB share answers `AccountChanged`: its edit is the HOST's
/// (`smb_hosts::edit_host`). A place nobody saved any more answers
/// `Unreachable`, as [`save_edit`] does.
pub(crate) async fn save_place_edit(id: &str, edit: PlaceEdit) -> SavedServerOutcome {
    let Some(saved) = saved_by_id(id) else {
        return SavedServerOutcome::Unreachable;
    };
    let PlaceEdit {
        display_name,
        remote_root,
        start_folder,
        uid,
        gid,
    } = edit;
    let target = match saved {
        SavedEntry::Sftp(entry) => ServerTarget::Sftp {
            display_name: display_name.unwrap_or(entry.display_name),
            host: entry.host,
            port: entry.port,
            username: entry.username,
            remote_root: remote_root.unwrap_or(entry.remote_root),
            start_folder,
            key_file: entry.key_file,
            use_agent: entry.use_agent,
            auto_reconnect: entry.auto_reconnect,
        },
        SavedEntry::Webdav(entry) => ServerTarget::Webdav {
            display_name: display_name.unwrap_or(entry.display_name),
            url: entry.url,
            username: entry.username,
            remote_root: remote_root.unwrap_or(entry.remote_root),
            start_folder,
            auto_reconnect: entry.auto_reconnect,
        },
        // A blank name leaves the account's alone (`s3_known_places::adopt_typed_name`).
        SavedEntry::S3(place) => ServerTarget::S3 {
            display_name: display_name.unwrap_or_default(),
            provider: place.provider,
            access_key_id: place.access_key_id,
            bucket: place.bucket,
            auto_reconnect: place.auto_reconnect,
        },
        SavedEntry::Nfs(place) => ServerTarget::Nfs {
            display_name: display_name.unwrap_or_default(),
            host: place.host,
            port: place.port,
            export: place.export,
            uid: uid.unwrap_or(place.uid),
            gid: gid.unwrap_or(place.gid),
            start_folder,
            auto_reconnect: place.auto_reconnect,
        },
        SavedEntry::Smb(_) => return SavedServerOutcome::AccountChanged,
    };
    save_edit(id, target).await
}

/// The saved entry a target describes.
enum Entry {
    Sftp(KnownSftpServer),
    Webdav(KnownWebdavServer),
    /// One place under an account, and the ACCOUNT's name the target carried
    /// (`s3_known_places::adopt_typed_name`).
    S3 {
        place: KnownS3Place,
        account_name: String,
    },
    /// One export on a server, and the SERVER's name the target carried
    /// (`nfs_known_places::adopt_typed_name`).
    Nfs {
        place: KnownNfsPlace,
        server_name: String,
    },
}

impl Entry {
    fn s3_place_id(&self) -> Option<String> {
        match self {
            Self::S3 { place, .. } => place.volume_id(),
            _ => None,
        }
    }

    fn nfs_place_id(&self) -> Option<String> {
        match self {
            Self::Nfs { place, .. } => Some(place.volume_id()),
            _ => None,
        }
    }
}

/// The store entry `server` describes.
///
/// ❗ `pinned: true` and a fresh `last_connected_at` are read only for a NEW
/// entry: `remember` keeps a stored pin, and a move keeps both.
fn entry_of(server: ServerTarget) -> Option<Entry> {
    let entry = match server {
        ServerTarget::Sftp {
            display_name,
            host,
            port,
            username,
            remote_root,
            start_folder,
            key_file,
            use_agent,
            auto_reconnect,
        } => Entry::Sftp(KnownSftpServer {
            host,
            port,
            username,
            display_name,
            remote_root,
            start_folder,
            key_file,
            use_agent,
            auto_reconnect,
            pinned: true,
            last_connected_at: chrono::Utc::now().to_rfc3339(),
        }),
        ServerTarget::Webdav {
            display_name,
            url,
            username,
            remote_root,
            start_folder,
            auto_reconnect,
        } => Entry::Webdav(KnownWebdavServer {
            url,
            username,
            display_name,
            remote_root,
            start_folder,
            auto_reconnect,
            pinned: true,
            last_connected_at: chrono::Utc::now().to_rfc3339(),
        }),
        ServerTarget::S3 {
            display_name,
            provider,
            access_key_id,
            bucket,
            auto_reconnect,
        } => Entry::S3 {
            place: s3_place(provider, access_key_id, bucket, auto_reconnect),
            account_name: display_name,
        },
        nfs @ ServerTarget::Nfs { .. } => {
            let (place, server_name) = nfs_place(nfs)?;
            Entry::Nfs { place, server_name }
        }
    };
    Some(entry)
}

#[cfg(test)]
#[path = "saves_test.rs"]
mod saves_test;
