#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an NFSv3 server: our own user-space client, so a
//! `secure` export opens with no root and no OS mount.

#[allow(
    dead_code,
    reason = "the protocol layer lands before the volume that calls it; drop this once the volume is wired"
)]
pub(crate) mod dial;
pub(crate) mod errors;
pub(crate) mod params;
#[allow(
    dead_code,
    reason = "the protocol layer lands before the volume that calls it; drop this once the volume is wired"
)]
pub(crate) mod proto;
#[allow(
    dead_code,
    reason = "the protocol layer lands before the volume that calls it; drop this once the volume is wired"
)]
pub(crate) mod transport;
pub(crate) mod volume;
#[allow(
    dead_code,
    reason = "the protocol layer lands before the volume that calls it; drop this once the volume is wired"
)]
pub(crate) mod xdr;

pub use errors::NfsConnectError;
pub use params::{NfsConnectionParams, NfsIdentity, NfsServer};
pub use volume::{NfsIdentityGuess, NfsIdentitySource, NfsVolume, connect_nfs_volume, guess_identity, list_exports};
