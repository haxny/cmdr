//! Package icon keys: a macOS package (`Safari.app`) shows its own composite
//! icon in Finder rather than a folder glyph, so it earns a real per-path fetch.
//!
//! Which folders are packages isn't decided here: the listing reads the name
//! through `file_extension::is_listed_package` (pure, no I/O, so it runs for
//! every entry and `entry::get_icon_id` routes a hit straight to a `pkg:` key),
//! and the app asks macOS about the visible dotted folders the list doesn't know
//! (`FileEntry::set_package`). This module only builds the key.
//!
//! The other Tier C signal, a folder carrying `kHasCustomIcon` in its
//! `com.apple.FinderInfo` xattr, needs a `getxattr` syscall and therefore stays
//! in the app, where it runs only for the bounded set of *visible* directory
//! paths the frontend asks about.

/// Prefix marking package icon keys (`pkg:/Applications/Safari.app`, …). Shares
/// the `path:`-key lifecycle (LRU-capped, not persisted): each package's icon is
/// its own, so they're as unbounded as custom-icon folders. The distinct prefix
/// keeps the two candidate sources legible in logs.
pub const PKG_KEY_PREFIX: &str = "pkg:";

/// The `pkg:{path}` icon key for the package at `path`. The key carries the full
/// path because each bundle's icon is distinct, so unlike `special:*` they can't
/// share a bounded key.
pub fn package_icon_id(path: &str) -> String {
    format!("{PKG_KEY_PREFIX}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_icon_id_uses_the_pkg_prefix_and_full_path() {
        assert_eq!(
            package_icon_id("/Applications/Safari.app"),
            "pkg:/Applications/Safari.app"
        );
    }
}
