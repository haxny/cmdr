//! Synchronize folders, the app's half: saved profiles today (`profile_store`), the
//! compare session and the engine adapter later. See `DETAILS.md`.

pub(crate) mod profile_store;

#[cfg(test)]
mod profile_store_test;
