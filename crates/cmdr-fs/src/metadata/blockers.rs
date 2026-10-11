//! Clearing what makes an item refuse a rename or an unlink: the user flags
//! `uchg`/`uappnd` and the ACL.
//!
//! Two callers want it. A landing whose rename was refused clears the staged
//! item, lands it, and puts back what it cleared ([`Unblock::Cleared`] hands the
//! set over). A reversal or a sweep about to remove an item Cmdr made clears it
//! so the removal can happen at all. Both ask only after a first attempt was
//! refused, so an ordinary item never reaches here.

use std::io;
use std::path::Path;

use super::LandedMetadata;

/// What [`clear_blockers`] did to one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unblock {
    /// It cleared these; put them back on the item where it lands to keep them.
    Cleared(LandedMetadata),
    /// The item wears nothing to clear (or is gone), so a retry would be refused
    /// the same way.
    NothingToClear,
    /// It wears a blocker this process can't clear (`schg`/`sappnd`, which only
    /// root can), or the clear itself was refused.
    Refused,
}

/// Clears `path`'s own `uchg`/`uappnd` and strips its ACL, flags first because
/// `uchg` refuses an ACL write. Never follows a symlink. On Linux there's never
/// anything to clear (module docs).
pub fn clear_blockers(path: &Path) -> Unblock {
    #[cfg(target_os = "macos")]
    {
        super::note_helper_call();
        clear_on_macos(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Unblock::NothingToClear
    }
}

#[cfg(target_os = "macos")]
fn clear_on_macos(path: &Path) -> Unblock {
    use super::{Acl, UserFlags, flags};

    let Ok(raw) = flags::read_raw(path) else {
        return Unblock::NothingToClear;
    };
    if raw & (libc::SF_IMMUTABLE | libc::SF_APPEND) != 0 {
        return Unblock::Refused;
    }
    let mut cleared = LandedMetadata::default();
    let blocking = UserFlags::from_bsd(raw).intersection(UserFlags::RENAME_BLOCKERS);
    if !blocking.is_empty() {
        if let Err(e) = flags::write_raw(path, raw & !blocking.to_bsd()) {
            log::debug!(target: "metadata", "couldn't unlock {}: {e}", path.display());
            return Unblock::Refused;
        }
        cleared.flags = blocking;
    }
    match exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL) {
        Ok(entries) if !entries.is_empty() => {
            if let Err(e) = exacl::setfacl(&[path], &[], exacl::AclOption::SYMLINK_ACL) {
                log::debug!(target: "metadata", "couldn't strip the ACL of {}: {e}", path.display());
                return Unblock::Refused;
            }
            cleared.acl = Some(Acl(entries));
        }
        Ok(_) => {}
        Err(e) => log::debug!(target: "metadata", "couldn't read the ACL of {}: {e}", path.display()),
    }
    if cleared.is_empty() {
        Unblock::NothingToClear
    } else {
        Unblock::Cleared(cleared)
    }
}

/// Why a `remove_*_clearing_blockers` call left the item in place.
#[derive(Debug)]
pub enum RemovalRefusal {
    /// The item wears a blocker this process can't clear ([`Unblock::Refused`]).
    Locked,
    /// The removal itself failed: the first refusal when there was nothing to
    /// clear, else the retry's.
    Io(io::Error),
}

/// `remove_file`, and when that's refused, [`clear_blockers`] and one retry.
pub fn remove_file_clearing_blockers(path: &Path) -> Result<(), RemovalRefusal> {
    remove_clearing(path, |p| std::fs::remove_file(p))
}

/// `remove_dir` (empty only), and when that's refused, [`clear_blockers`] and
/// one retry.
pub fn remove_dir_clearing_blockers(path: &Path) -> Result<(), RemovalRefusal> {
    remove_clearing(path, |p| std::fs::remove_dir(p))
}

fn remove_clearing(path: &Path, remove: impl Fn(&Path) -> io::Result<()>) -> Result<(), RemovalRefusal> {
    match remove(path) {
        Ok(()) => Ok(()),
        Err(first) if first.kind() == io::ErrorKind::PermissionDenied => match clear_blockers(path) {
            Unblock::Cleared(_) => remove(path).map_err(RemovalRefusal::Io),
            Unblock::NothingToClear => Err(RemovalRefusal::Io(first)),
            Unblock::Refused => Err(RemovalRefusal::Locked),
        },
        Err(e) => Err(RemovalRefusal::Io(e)),
    }
}

/// `remove_dir_all`, and when that fails, clearing every entry left in the tree
/// (and the root) before one retry. Recursing only after a refusal keeps an
/// ordinary tree at exactly the cost of `remove_dir_all`.
///
/// ❗ Only for a tree Cmdr made and owns whole (a cross-filesystem move's
/// staging folder): it clears and removes everything in it.
pub fn remove_tree_clearing_blockers(root: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(root) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(e),
        Err(first) => {
            if !clear_tree(root) {
                return Err(first);
            }
            std::fs::remove_dir_all(root)
        }
    }
}

/// Clears every entry under `dir` and `dir` itself, never following a link.
/// Whether it cleared anything.
fn clear_tree(dir: &Path) -> bool {
    // The folder first, so a Locked folder's children can then be read past it
    // and removed out of it.
    let mut cleared = matches!(clear_blockers(dir), Unblock::Cleared(_));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                cleared |= clear_tree(&path);
            } else {
                cleared |= matches!(clear_blockers(&path), Unblock::Cleared(_));
            }
        }
    }
    cleared
}
