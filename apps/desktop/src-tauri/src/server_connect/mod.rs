//! Connecting a place the user saved, below every caller: the `connect_saved_place`
//! and `cancel_server_connect` commands (`commands::servers`, thin IPC over this) and
//! Ask Cmdr's `connect_volume` (`agent::tools::connect`).
//!
//! - `dial.rs`: one flight per place (`network::dial_flights`), then the dial by the
//!   saved entry's protocol. A saved place registers its volume only once its dial
//!   lands, so the "already registered?" refusal can't guard the 30 s a dial is out;
//!   the flight can.
//! - `outcome.rs`: `ServerConnectOutcome` and `SavedPlaceRefusal`, the wire
//!   vocabulary every caller branches on, plus each wiring's outcome widened into it.
//! - `saved.rs`: the saved entry a place's volume id names, across every store.

mod dial;
pub(crate) mod outcome;
mod saved;

pub(crate) use dial::{SavedPlaceDial, cancel_connect, connect_saved_place};
pub use outcome::{SavedPlaceRefusal, ServerConnectOutcome};
pub(crate) use outcome::{outcome_from_nfs, outcome_from_s3, outcome_from_sftp, outcome_from_webdav};
pub(crate) use saved::{SavedEntry, saved_by_id, webdav_params};
