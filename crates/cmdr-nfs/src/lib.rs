#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an NFSv3 server: our own user-space client, so a
//! `secure` export opens with no root and no OS mount.

pub(crate) mod errors;
pub(crate) mod params;
pub(crate) mod volume;

pub use errors::NfsConnectError;
pub use params::{NfsConnectionParams, NfsIdentity, NfsServer};
pub use volume::{NfsIdentityGuess, NfsIdentitySource, NfsVolume, connect_nfs_volume, guess_identity, list_exports};
