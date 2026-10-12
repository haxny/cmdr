//! Which transient files a listing leaves out of the pane.
//!
//! Cmdr writes a file under a temporary name and renames it once the last byte
//! lands, so a crash can never leave a half-written file at a real name
//! (`write_operations/transfer/staged_write.rs`). The mint itself, the naming
//! convention, and the in-flight registry are `cmdr_fs::staging`'s, so a backend
//! crate can stage a write without reaching into the app; everything re-exported
//! below keeps its `crate::file_system::staging::…` path. So are the pure name
//! rules over them (`could_be_hidden_from_listings`, `hidden_with_settings`), so
//! the listing engine asks them without the app. What's left here is the part
//! that belongs to the product: the two settings that decide whether the user
//! sees them.
//!
//! The cost of staging is that a copy makes files appear under names like
//! `photo.jpg.cmdr-tmp-3f2a…` for as long as it takes to write them, and a
//! directory watcher happily reports those to the pane. macOS apps do the same
//! thing when they save (`file.txt.sb-1e64c894-…`), so the pane has two kinds of
//! other people's scratch to deal with.
//!
//! ## Two categories, two rules, and why they differ
//!
//! **Cmdr's own** (`.cmdr-tmp-*`, `.cmdr-temp-*`, `.cmdr-staging-*`) hides by OWNERSHIP. One a
//! running operation put there is noise that'll be gone in moments; one nobody
//! owns is a LEFTOVER from an interrupted transfer, and hiding that would be
//! lying about what's on disk — and about a Cmdr bug worth seeing. So:
//!
//! > Hide a scratch file while an operation has it open. Show every other one.
//!
//! **Other apps'** (`.sb-`, macOS safe-save) hides by NAME, because we have no
//! ownership signal for a file another process is writing and no way to tell a
//! live one from an abandoned one. The coarser rule is defensible here precisely
//! because it isn't ours: an abandoned `.sb-` is evidence of TextEdit's day, not
//! of a Cmdr bug, so nothing is lost by not surfacing it. ❌ Don't reach for this
//! rule for Cmdr's own scratch, where ownership is knowable and the leftover is
//! the interesting case. The ownership signal itself (the RAII mint, the weak
//! liveness token behind it, and why a guard alone isn't enough) is documented
//! where it lives, in `cmdr_fs::staging`.

pub use cmdr_fs::staging::StagingTemp;

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::staging::{hidden_with_settings, is_staging_temp_in_flight};
    use std::path::Path;
    use std::sync::Arc;

    /// Stands in for a running operation: the temps it mints stay hidden until
    /// the returned `Arc` is dropped, which is what settling does for real.
    fn running_operation() -> Arc<()> {
        Arc::new(())
    }

    /// Whether `name` stays out of the pane under `(show_staging_temps,
    /// show_safe_save_files)`.
    fn hidden(name: &str, (show_staging, show_safe_save): (bool, bool)) -> bool {
        hidden_with_settings(name, show_staging, show_safe_save)
    }

    fn name_of(temp: &StagingTemp) -> String {
        temp.path()
            .file_name()
            .expect("a minted temp always has a file name")
            .to_string_lossy()
            .into_owned()
    }

    /// The whole point of the guard: while it lives the file is hidden, and the
    /// moment it drops it isn't.
    #[test]
    fn a_live_guard_hides_its_temp_and_dropping_it_stops() {
        let settings = (false, true);
        let op = running_operation();
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), Some(Arc::downgrade(&op)));
        let name = name_of(&temp);

        assert!(hidden(&name, settings));
        drop(temp);
        assert!(!hidden(&name, settings));
    }

    /// David's first edge case: an operation that wedges and leaves its temps on
    /// disk must not leave them hidden too. Its guard may never drop — the driver
    /// abandons a task that won't wind down, and the task keeps holding it — so
    /// the operation's liveness is what has to answer.
    #[test]
    fn a_leftover_from_a_dead_operation_is_visible_even_if_its_guard_leaked() {
        let settings = (false, true);
        let op = running_operation();
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), Some(Arc::downgrade(&op)));
        let name = name_of(&temp);
        assert!(hidden(&name, settings), "hidden while the operation runs");

        drop(op);

        // The guard deliberately stays alive, standing in for the abandoned task.
        assert!(
            !hidden(&name, settings),
            "a leftover nobody is running for must be visible"
        );
        drop(temp);
    }

    /// A cross-filesystem move's staging folder is Cmdr's scratch too, and it
    /// sits in the destination the user is watching for the whole move. It hides
    /// by the same ownership rule, so one left behind by an interrupted move
    /// shows up rather than lurking.
    #[test]
    fn a_moves_staging_directory_hides_while_the_move_owns_it() {
        let settings = (false, true);
        let op = running_operation();
        let name = format!(".cmdr-staging-{}", uuid::Uuid::new_v4());
        let guard = StagingTemp::adopt(Path::new("/dest").join(&name), Some(Arc::downgrade(&op)));

        assert!(hidden(&name, settings), "hidden while the move runs");
        drop(guard);
        assert!(
            !hidden(&name, settings),
            "a staging folder nobody owns is a leftover the user must see"
        );
    }

    /// Ordinary files are never touched.
    #[test]
    fn an_ordinary_file_is_never_hidden() {
        let settings = (false, true);
        let _temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        assert!(!hidden("photo.jpg", settings));
        assert!(!hidden(".gitignore", settings));
    }

    /// Safe-save scratch shows by default: it's another app's business, and
    /// hiding someone else's files by name is a bigger claim than hiding our own.
    #[test]
    fn safe_save_scratch_is_shown_by_default() {
        let settings = (false, true);
        assert!(!hidden("notes.txt.sb-1e64c894-vFWIzN", settings));
    }

    /// Turning the setting off hides it purely by name: no operation owns another
    /// app's scratch, so there's nothing else to go on.
    #[test]
    fn safe_save_scratch_hides_by_name_alone() {
        let settings = (false, false);
        assert!(hidden("notes.txt.sb-1e64c894-vFWIzN", settings));
        assert!(
            hidden("notes.txt.sb-1e64c894", settings),
            "the temp DIRECTORY hides too, not just the file inside it"
        );
        assert!(!hidden("notes.txt", settings));
    }

    /// The two categories are independent switches, so neither setting can
    /// silently change what the other one does.
    #[test]
    fn the_two_scratch_settings_are_independent() {
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        let cmdr_name = name_of(&temp);
        let safe_save_name = "notes.txt.sb-1e64c894-vFWIzN";

        {
            let settings = (true, false);
            assert!(!hidden(&cmdr_name, settings), "Cmdr scratch shown");
            assert!(hidden(safe_save_name, settings), "safe-save hidden");
        }
        {
            let settings = (false, true);
            assert!(hidden(&cmdr_name, settings), "Cmdr scratch hidden");
            assert!(!hidden(safe_save_name, settings), "safe-save shown");
        }
    }

    /// The Settings > Advanced escape hatch overrides the hiding, not the
    /// registry: what's in flight is still in flight, it's just shown.
    #[test]
    fn the_setting_shows_in_flight_temps() {
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        let name = name_of(&temp);

        let settings = (true, true);
        assert!(!hidden(&name, settings));
        assert!(is_staging_temp_in_flight(&name), "still in flight, just not hidden");
    }
}
