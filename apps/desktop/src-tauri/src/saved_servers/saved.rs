//! The saved entry a place's volume id names, across every server store.

use cmdr_webdav::WebdavConnectionParams;

use crate::network::{known_shares, nfs_known_places, s3_known_places, sftp_known_servers, webdav_known_servers};

/// A saved entry, found by the id its place carries.
pub(crate) enum SavedEntry {
    Sftp(sftp_known_servers::KnownSftpServer),
    Webdav(webdav_known_servers::KnownWebdavServer),
    /// One saved S3 place (a bucket, or an account root).
    S3(s3_known_places::KnownS3Place),
    /// One saved NFS place (an export).
    Nfs(nfs_known_places::KnownNfsPlace),
    /// A saved SMB share whose last mount had that id.
    Smb(known_shares::KnownNetworkShare),
}

/// The saved entry whose derived volume id is `volume_id`.
///
/// ❗ Derived rather than stored: the id funnel (`cmdr_fs::volume::ids`) is the
/// one place an id is minted, so looking one up means re-deriving it from the
/// same tuple rather than keeping a second copy that can drift.
pub(crate) fn saved_by_id(volume_id: &str) -> Option<SavedEntry> {
    let sftp = sftp_known_servers::all()
        .into_iter()
        .find(|entry| cmdr_fs::volume::sftp_volume_id(&entry.host, entry.port, &entry.username) == volume_id);
    if let Some(entry) = sftp {
        return Some(SavedEntry::Sftp(entry));
    }
    let webdav = webdav_known_servers::all().into_iter().find(|entry| {
        webdav_params(&entry.url, &entry.username, "/").is_some_and(|params| {
            cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &entry.username) == volume_id
        })
    });
    if let Some(entry) = webdav {
        return Some(SavedEntry::Webdav(entry));
    }
    if let Some(entry) = s3_known_places::find(volume_id) {
        return Some(SavedEntry::S3(entry));
    }
    if let Some(entry) = nfs_known_places::find(volume_id) {
        return Some(SavedEntry::Nfs(entry));
    }
    // ❗ Stored, ❌ not derived, unlike the two above: an SMB id comes off the
    // mount's `statfs`, and only the row knows which spelling the mount got.
    known_shares::share_by_volume_id(volume_id).map(SavedEntry::Smb)
}

/// Connection params for a saved WebDAV entry, or `None` when its address isn't
/// an `http`/`https` URL.
pub(crate) fn webdav_params(url: &str, username: &str, remote_root: &str) -> Option<WebdavConnectionParams> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| WebdavConnectionParams::new(parsed, username, remote_root))
}
