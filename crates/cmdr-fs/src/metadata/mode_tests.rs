//! The routed-mode fold, on its own: what a landed item wears given what the
//! source reported and what a fresh item gets under the user's umask.

use super::*;

/// The reason the fold exists, at the smallest scale: an executable source
/// lands executable under the umask everybody has.
#[test]
fn an_executable_source_lands_executable() {
    assert_eq!(routed_mode(0o755, 0o644), Some(0o755));
}

/// A source with no mode to report (the `FileEntry::permissions` sentinel of
/// every backend without a permission concept) leaves the item as created.
/// ❌ Never a `chmod` to `0`, which would land a file nobody can open.
#[test]
fn a_source_with_no_permission_concept_leaves_the_item_alone() {
    assert_eq!(routed_mode(0, 0o644), None);
}

/// The umask is honored: under `077` an executable source lands `0o700`, what
/// `git checkout` of the same blob would produce, ❌ never a world-readable
/// `0o755`.
#[test]
fn a_strict_umask_narrows_what_lands_the_way_a_checkout_would() {
    assert_eq!(routed_mode(0o755, 0o600), Some(0o700));
    assert_eq!(routed_mode(0o644, 0o600), Some(0o600));
}

/// A group-writable umask (`002`) doesn't stop a `0o755` source from landing
/// exactly `0o755`: the umask only ever takes bits away.
#[test]
fn a_permissive_umask_still_lands_the_mode_the_source_named() {
    assert_eq!(routed_mode(0o755, 0o664), Some(0o755));
    assert_eq!(routed_mode(0o664, 0o664), Some(0o664));
}

/// The Windows-zip case: the `0o666` stand-in a DOS creator's attributes turn
/// into folds to the plain mode a new file would have had anyway.
#[test]
fn the_stand_in_mode_a_windows_zip_reports_lands_where_a_plain_new_file_would() {
    assert_eq!(routed_mode(0o666, 0o644), Some(0o644));
    assert_eq!(routed_mode(0o666, 0o600), Some(0o600));
}

/// A private source stays private.
#[test]
fn a_private_source_lands_private() {
    assert_eq!(routed_mode(0o600, 0o644), Some(0o600));
}

/// setuid, setgid, and sticky never travel on a routed copy: an archive or a
/// repo object isn't authority enough for one of those bits.
#[test]
fn setuid_setgid_and_sticky_never_travel() {
    assert_eq!(routed_mode(0o4755, 0o644), Some(0o755));
    assert_eq!(routed_mode(0o2755, 0o644), Some(0o755));
    assert_eq!(routed_mode(0o1777, 0o644), Some(0o755));
}

/// A full `st_mode` (file-type bits and all) is masked to its permission bits.
#[test]
fn the_file_type_bits_a_stat_carries_are_masked_off() {
    assert_eq!(routed_mode(0o100_755, 0o644), Some(0o755));
}

/// A folder's default keeps execute where read survived, so a `0o700` source
/// folder lands `0o700` and a `0o777` one lands at the folder default.
#[test]
fn a_folder_folds_against_the_folder_default() {
    assert_eq!(routed_mode(0o700, 0o755), Some(0o700));
    assert_eq!(routed_mode(0o777, 0o755), Some(0o755));
}

/// The probe reads the umask this test process runs under: a new file's
/// default is never wider than `0o666`, and the folder default adds execute
/// exactly where the file default has read.
#[test]
fn the_defaults_agree_with_each_other() {
    let file = default_file_mode();
    let folder = default_folder_mode();
    assert_eq!(file & !0o666, 0);
    assert_eq!(folder & 0o666, file);
    assert_ne!(file & 0o600, 0, "the owner can always read and write a new file");
}
