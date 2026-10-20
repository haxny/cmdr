#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an NFSv3 server: our own user-space client, so a
//! `secure` export opens with no root and no OS mount.

pub(crate) mod dial;
pub(crate) mod errors;
pub(crate) mod params;
#[allow(
    dead_code,
    reason = "WRITE, CREATE, COMMIT, and the other mutations are the write side's to call; drop this once it lands"
)]
pub(crate) mod proto;
#[allow(
    dead_code,
    reason = "`lost` and `is_usable` are the outage side's (the poller polls only a usable connection); `Pending::procedure` feeds the test tap"
)]
pub(crate) mod transport;
pub(crate) mod volume;
#[allow(
    dead_code,
    reason = "`i32`, `remaining`, and `rest` are the codec's whole vocabulary, which the tests and the fake server speak"
)]
pub(crate) mod xdr;

pub use errors::NfsConnectError;
pub use params::{NfsConnectionParams, NfsIdentity, NfsServer};
pub use volume::{NfsIdentityGuess, NfsIdentitySource, NfsVolume, connect_nfs_volume, guess_identity, list_exports};

/// The Docker fixture's connection helpers, for the app's NFS suites. Exists
/// only under the `testing` feature.
#[cfg(feature = "testing")]
pub use volume::testing;
