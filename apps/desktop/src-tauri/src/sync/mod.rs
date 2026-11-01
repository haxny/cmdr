//! Synchronize folders, the app's half: saved profiles (`profile_store`), the
//! compare and run sessions (`session*`), and the `SyncEngine` adapter over the
//! write engine (`engine*`). See `DETAILS.md`.

pub(crate) mod engine;
mod engine_ops;
pub(crate) mod host;
pub(crate) mod profile_store;
pub(crate) mod run_lock;
pub(crate) mod session;
pub(crate) mod session_compare;
pub(crate) mod session_run;
pub(crate) mod settle_sink;
pub(crate) mod wire;

#[cfg(test)]
mod engine_test;
#[cfg(test)]
mod profile_store_test;
#[cfg(test)]
mod session_test;
