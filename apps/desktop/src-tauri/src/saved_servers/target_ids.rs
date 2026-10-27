//! The id a [`ServerTarget`] names, through the same id funnel the servers
//! listing uses (`cmdr_fs::volume::ids`). Behind `saved_server_id`.

use super::s3_accounts::s3_place;
use super::webdav_params;
use super::wire::ServerTarget;

/// The place id `server` names, or `None` for a WebDAV address that isn't an
/// `http`/`https` URL (and an S3 target whose provider makes no endpoint).
pub(crate) fn place_id(server: ServerTarget) -> Option<String> {
    match server {
        ServerTarget::Sftp {
            host, port, username, ..
        } => Some(cmdr_fs::volume::sftp_volume_id(&host, port, &username)),
        ServerTarget::Webdav { url, username, .. } => {
            let params = webdav_params(&url, &username, "/")?;
            Some(cmdr_fs::volume::webdav_volume_id(
                params.host(),
                params.port(),
                &username,
            ))
        }
        ServerTarget::S3 {
            provider,
            access_key_id,
            bucket,
            ..
        } => s3_place(provider, access_key_id, bucket, true).volume_id(),
        ServerTarget::Nfs {
            host,
            port,
            export,
            uid,
            gid,
            ..
        } => {
            let server = cmdr_nfs::NfsServer::new(&host, port);
            Some(cmdr_nfs::NfsConnectionParams::new(server, &export, cmdr_nfs::NfsIdentity { uid, gid }).volume_id())
        }
    }
}
