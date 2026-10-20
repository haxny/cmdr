use super::*;

#[test]
fn a_place_with_no_port_is_minted_on_the_nfs_port() {
    // Identity can't wait for portmap: a saved place must name one id before
    // anything is dialed, and the same one after.
    let params = NfsConnectionParams::new(
        NfsServer::new("NAS.local", None),
        "/Public/",
        NfsIdentity { uid: 1000, gid: 100 },
    );
    assert_eq!(params.export(), "/Public");
    assert_eq!(
        params.volume_id(),
        cmdr_fs::volume::nfs_volume_id("nas.local", NFS_PORT, "/Public")
    );
    assert_eq!(params.app_root(), "nfs://nas.local:2049/Public");
}

#[test]
fn the_identity_never_reaches_the_volume_id() {
    let as_ada = NfsConnectionParams::new(
        NfsServer::new("nas", Some(2049)),
        "/x",
        NfsIdentity { uid: 1000, gid: 100 },
    );
    let as_bob = NfsConnectionParams::new(
        NfsServer::new("nas", Some(2049)),
        "/x",
        NfsIdentity { uid: 1001, gid: 100 },
    );
    assert_eq!(as_ada.volume_id(), as_bob.volume_id());
}
