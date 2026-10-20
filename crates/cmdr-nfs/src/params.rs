//! How to reach one NFS export, and how to reach it again.

/// The port an NFS server listens on when nothing says otherwise, and the port
/// a volume id and an app path carry when the address named none.
pub(crate) const NFS_PORT: u16 = 2049;

/// One NFS server: the ACCOUNT an export is a place under.
///
/// ❗ No secret lives here, and none exists: `AUTH_SYS` sends a uid and a gid,
/// which is why a place connects with nothing in the credential store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsServer {
    /// The host, as the person typed it: a name or an IPv4 or IPv6 literal.
    pub host: String,
    /// The NFS port the address named. `None` asks the portmapper, falling back
    /// to 2049 when it has no answer; an explicit one skips the NFS lookup.
    pub port: Option<u16>,
    /// Where the portmapper answers, when not on 111. Only a Docker fixture sets
    /// it: its `rpcbind` is published on a high port. MOUNT has no well-known
    /// port, so it always comes from the portmapper.
    pub portmap_port: Option<u16>,
}

impl NfsServer {
    /// A server found the standard way: portmap on 111 names the MOUNT port.
    pub fn new(host: &str, port: Option<u16>) -> Self {
        Self {
            host: host.to_string(),
            port,
            portmap_port: None,
        }
    }

    /// The port identity is minted from: the one the address named, else 2049.
    /// ❗ Never the port portmap answered, which can move when the server
    /// restarts and would orphan every tab on the place.
    pub fn identity_port(&self) -> u16 {
        self.port.unwrap_or(NFS_PORT)
    }
}

/// Who a place acts as on the server: the uid and gid every call's `AUTH_SYS`
/// credential carries.
///
/// ❗ A SETTING of the place, ❌ not its identity: changing it keeps the volume
/// id, the tabs, and the favorites, and redials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NfsIdentity {
    /// The user id.
    pub uid: u32,
    /// The primary group id.
    pub gid: u32,
}

/// Everything needed to open (and later re-open) one NFS export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsConnectionParams {
    /// The server the export is on.
    pub server: NfsServer,
    /// The export, canonical (`cmdr_fs::volume::canonical_nfs_export`). Private
    /// so nothing stores a second spelling of it.
    export: String,
    /// Who the place acts as.
    pub identity: NfsIdentity,
    /// Where a pane lands when the place is opened: an export-relative folder
    /// (`/Photos`). `None` is the export's root.
    pub start_folder: Option<String>,
    /// Whether Cmdr may redial unattended when the connection drops. The user's
    /// own per-place switch.
    pub auto_reconnect: bool,
}

impl NfsConnectionParams {
    /// Params for the common case: the export's root, and a dropped connection
    /// comes back on its own.
    ///
    /// `export` may be what a person typed, folder and all
    /// (`/share/Public/photos`): [`connect_nfs_volume`](crate::connect_nfs_volume)
    /// splits it against the server's exports, and the volume it returns
    /// carries the resolved split ([`NfsVolume::params`](crate::NfsVolume::params)).
    pub fn new(server: NfsServer, export: &str, identity: NfsIdentity) -> Self {
        Self {
            server,
            export: cmdr_fs::volume::canonical_nfs_export(export),
            identity,
            start_folder: None,
            auto_reconnect: true,
        }
    }

    /// The export this place is, canonical.
    pub fn export(&self) -> &str {
        &self.export
    }

    /// The id this place registers under (`cmdr_fs::volume::nfs_volume_id`).
    pub fn volume_id(&self) -> String {
        cmdr_fs::volume::nfs_volume_id(&self.server.host, self.server.identity_port(), &self.export)
    }

    /// The app path this place's root is addressed by
    /// (`cmdr_fs::volume::nfs_app_root`).
    pub fn app_root(&self) -> String {
        cmdr_fs::volume::nfs_app_root(&self.server.host, self.server.identity_port(), &self.export)
    }
}

#[cfg(test)]
#[path = "params_test.rs"]
mod params_test;
