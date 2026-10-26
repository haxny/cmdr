//! The Tags tab of Multi-Rename: a tag session per Multi-Rename session, edited in memory.
//! See `CLAUDE.md`.

#![allow(dead_code, reason = "the IPC commands that call this module land in the next task")]

pub(crate) mod apply;
pub(crate) mod session;
pub(crate) mod undo;
pub(crate) mod view;

#[cfg(test)]
mod apply_test;
#[cfg(test)]
mod session_test;
#[cfg(test)]
mod view_test;
