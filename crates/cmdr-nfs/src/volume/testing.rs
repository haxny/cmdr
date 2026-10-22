//! Fixtures for the Docker-backed NFS suites, on both sides of the crate
//! boundary. Gated behind the `testing` feature, so it exists in dev targets
//! and in no shipped build. The stack itself, its seeded tree, and why
//! `/secure` is reachable only from a Linux host:
//! `apps/desktop/test/nfs-servers/README.md`.
//!
//! ❗ The portmap port comes from `NFS_FIXTURE_PORTMAP_PORT` HERE, in test
//! code, ❌ never in the crate proper: a real server's portmapper is on 111.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use tokio_util::sync::CancellationToken;

use super::{NfsVolume, connect_nfs_volume};
use crate::errors::NfsConnectError;
use crate::params::{NfsConnectionParams, NfsIdentity, NfsServer};

/// The export any port may reach.
pub const FIXTURE_INSECURE_EXPORT: &str = "/insecure";
/// The export that refuses a source port past 1023 (knfsd's default `secure`).
pub const FIXTURE_SECURE_EXPORT: &str = "/secure";

/// Who owns the seeded tree, and who a cell acts as: root-squash leaves the
/// files to uid 1000.
pub const FIXTURE_IDENTITY: NfsIdentity = NfsIdentity { uid: 1000, gid: 1000 };

/// The file every fixture carries for the byte path to read.
pub const FIXTURE_LARGE_FILE: &str = "large.bin";
/// Its length: 200 MiB (`LARGE_MB` in `seed.sh`).
pub const FIXTURE_LARGE_LEN: u64 = 200 * 1024 * 1024;

fn env_port(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// The fixture server through its published loopback ports.
pub fn fixture_server() -> NfsServer {
    NfsServer {
        host: "127.0.0.1".to_string(),
        port: None,
        portmap_port: Some(env_port("NFS_FIXTURE_PORTMAP_PORT", 15480)),
    }
}

/// The fixture server dialed at the container's own IP, where the source port
/// survives: set by the integration lane on Linux only. `None` everywhere else,
/// which is every macOS run.
pub fn fixture_direct_server() -> Option<NfsServer> {
    let host = std::env::var("NFS_FIXTURE_DIRECT_HOST").ok()?;
    Some(NfsServer {
        host,
        port: None,
        portmap_port: None,
    })
}

/// Params for `export` (or a path under it) on the fixture, as the seeded
/// tree's owner.
pub fn fixture_params(export: &str) -> NfsConnectionParams {
    NfsConnectionParams::new(fixture_server(), export, FIXTURE_IDENTITY)
}

/// A host whose background work runs on the caller's runtime.
pub fn fixture_host() -> VolumeHost {
    VolumeHost::builder().runtime(tokio::runtime::Handle::current()).build()
}

/// Connects to `params` the way the app does.
pub async fn connect(params: NfsConnectionParams) -> Result<NfsVolume, NfsConnectError> {
    connect_nfs_volume("nfs-fixture", params, fixture_host(), CancellationToken::new()).await
}

/// Connects to the fixture's `/insecure` export.
pub async fn connect_insecure() -> NfsVolume {
    connect(fixture_params(FIXTURE_INSECURE_EXPORT))
        .await
        .unwrap_or_else(|err| panic!("the NFS fixture must be up (nfs-servers/start.sh): {err:?}"))
}

/// `remote` (`/photos`) as the app addresses it on `volume`.
pub fn app_path(volume: &NfsVolume, remote: &str) -> PathBuf {
    let remote = remote.trim_start_matches('/');
    if remote.is_empty() {
        return volume.root().to_path_buf();
    }
    Path::new(volume.root()).join(remote)
}

/// A fresh folder of the cell's own at the export's root, already created:
/// every cell shares the one export, and `nextest` runs them side by side.
/// `what` names the cell, so a mess left behind says whose it is.
pub async fn scratch_dir(volume: &NfsVolume, what: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let dir = app_path(volume, &format!("/scratch-{what}-{}-{nanos}", std::process::id()));
    volume
        .create_directory(&dir)
        .await
        .unwrap_or_else(|err| panic!("the scratch folder for {what} must be creatable: {err:?}"));
    dir
}

/// Removes `path` and everything under it, deepest first, best effort.
pub fn clean_deep<'a>(
    volume: &'a NfsVolume,
    path: &'a Path,
) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if let Ok(entries) = volume.list_directory(path, None).await {
            for entry in entries {
                let child = path.join(&entry.name);
                if entry.is_directory && !entry.is_symlink {
                    clean_deep(volume, &child).await;
                } else {
                    let _ = volume.delete(&child).await;
                }
            }
        }
        let _ = volume.delete(path).await;
    })
}

/// What `large.bin` holds at `[offset, offset + len)`: every 16-byte line its
/// own zero-padded number.
pub fn fixture_large_bytes_at(offset: u64, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 16);
    let mut line = offset / 16;
    let skip = (offset % 16) as usize;
    while out.len() < len + skip {
        out.extend_from_slice(format!("{line:015}\n").as_bytes());
        line += 1;
    }
    out.drain(..skip);
    out.truncate(len);
    out
}
