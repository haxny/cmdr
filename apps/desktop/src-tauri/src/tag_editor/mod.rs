//! The Tags tab of Multi-Rename: a tag session per Multi-Rename session, edited in memory.
//! See `CLAUDE.md`.

pub(crate) mod activity;
pub(crate) mod apply;
pub(crate) mod cover;
pub(crate) mod cover_choices;
pub(crate) mod playback;
pub(crate) mod session;
pub(crate) mod text_tools;
pub(crate) mod undo;
pub(crate) mod view;
pub(crate) mod window;

#[cfg(test)]
mod apply_test;
#[cfg(test)]
mod big_book_test;
#[cfg(test)]
mod cover_choices_test;
#[cfg(test)]
mod cover_test;
#[cfg(test)]
mod kind_test;
#[cfg(test)]
mod playback_test;
#[cfg(test)]
mod session_test;
#[cfg(test)]
mod text_tools_test;
#[cfg(test)]
mod view_test;
#[cfg(test)]
mod window_test;
