//! Names in ordinary prose: an absolute path under any prefix, and a path's leaf repeated bare
//! elsewhere on its line.

use super::*;

const TEST_PROCESS_SECRET: [u8; 32] = [0x7c; 32];

fn context() -> RedactionContext {
    RedactionContext::for_test(TEST_PROCESS_SECRET, "ERR-PROSE")
}

fn r(s: &str) -> String {
    redact_line(s).into_owned()
}

/// Hundreds of sites log `{}` of `path.display()`, and a server, an archive, or a phone names
/// paths under prefixes no dedicated branch knows (`/srv`, `/mnt`, `/DCIM`, `/sdcard`).
#[test]
fn an_absolute_path_under_any_prefix_is_redacted() {
    let cases = [
        (
            "Cover: can't walk /srv/clients/Acme Corp/plans.pdf: permission denied",
            "Cover: can't walk /<dir>/<dir>/<dir>/<file>.pdf: permission denied",
        ),
        (
            "SftpVolume::create_directory_all: /data/Secret Project",
            "SftpVolume::create_directory_all: /<dir>/<dir>",
        ),
        (
            "skipping too-small image '/mnt/backup/Anna Kovacs/tax.pdf' (12x12)",
            "skipping too-small image '/mnt/<dir>/<dir>/<file>.pdf' (12x12)",
        ),
        (
            "Upload failed for /DCIM/Camera/IMG_0042.HEIC on the phone: busy",
            "Upload failed for /<dir>/<dir>/<file>.HEIC on the phone: busy",
        ),
        (
            "Installing update into bundle: /Applications/Cmdr.app",
            "Installing update into bundle: /Applications/<file>.app",
        ),
        (
            "Phases: covering /Library/Secret Corp (Full)",
            "Phases: covering /Library/<dir> (Full)",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// A slash inside a word, a unit, a ratio, or a lone root isn't a path.
#[test]
fn slashes_that_are_not_absolute_paths_stay() {
    for input in [
        "copied 12 MB/s over TCP/IP",
        "read and/or write 3/4 of it",
        "MustScanSubDirs: reconcile slow for / (+38 -0 ~515676, 1691s)",
        "apps/desktop/src-tauri/src/lib.rs:12:5",
        "$HOME/Documents/<file:0123456789ab>.pdf",
    ] {
        assert_eq!(r(input), input, "should be unchanged: {input:?}");
    }
}

/// macOS repeats the file's name in its own error text, so the Trash line carried the name
/// twice: once in the path, which was redacted, and once bare, which shipped.
#[test]
fn a_path_leaf_repeated_bare_on_its_line_is_scrubbed() {
    let context = context();
    let input = "op 01a0 (Trash) failed: /Users/kajotac/Desktop/Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg: \
                 the Trash refused it: “Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg”";
    let out = context.redact_line(input).into_owned();
    assert!(!out.contains("Screenshot") && !out.contains("01.13.03"), "{out}");
    let leaf = context.redact_name("Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg", false);
    assert_eq!(out.matches(&leaf).count(), 2, "both copies share one token: {out}");
    assert!(out.ends_with(&format!("the Trash refused it: “{leaf}”")), "{out}");
}

/// The echo comes first just as often (`couldn't open "x" at /path/x`).
#[test]
fn a_leaf_named_before_its_path_is_scrubbed_too() {
    let out = r("couldn't open \"Budget 2026.xlsx\" at /Volumes/Data/Budget 2026.xlsx, giving up");
    assert_eq!(
        out,
        "couldn't open \"<file>.xlsx\" at /Volumes/<volume>/<file>.xlsx, giving up"
    );
}

/// Only whole-word repeats go, and only of names long enough not to be part of prose.
#[test]
fn the_leaf_scrub_leaves_other_words_alone() {
    assert_eq!(
        r("plans.pdf vs myplans.pdf in /srv/x/plans.pdf"),
        "<file>.pdf vs myplans.pdf in /<dir>/<dir>/<file>.pdf"
    );
    assert_eq!(r("ab in /srv/x/ab"), "ab in /<dir>/<dir>/<dir>");
}

/// A log target shares words with folder names; `_` and `::` glue them into one identifier.
#[test]
fn the_leaf_scrub_spares_log_targets() {
    assert_eq!(
        r("10:19:34.908 DEBUG media_index  media walk stopped in /srv/x/media"),
        "10:19:34.908 DEBUG media_index  <dir> walk stopped in /<dir>/<dir>/<dir>"
    );
    assert_eq!(
        r("10:19:34.908 DEBUG indexing::writer  writer stalled on /srv/x/writer"),
        "10:19:34.908 DEBUG indexing::writer  <dir> stalled on /<dir>/<dir>/<dir>"
    );
}

/// A bracket pair inside a folder name is part of the name. The forward scan once read the `]`
/// of `[§ 12 AB]` as the end of a sentence, so the token stopped there and the rest of the folder
/// name shipped in every listing, transfer, and verifier line (ERR-8MEX6). Only a closing bracket
/// with no opener in its own segment ends a path (`(pane sent to /Volumes/x/y); following`).
#[test]
fn a_bracket_pair_inside_a_name_keeps_the_name_whole() {
    let cases = [
        (
            "listing_done path=/Volumes/Team Share/1234-56.789 Acme Widget Co [§ 12 AB] 45X6_2020 QWERTY entries=241 read_dir_ms=257",
            "listing_done path=/Volumes/<volume>/<dir> entries=241 read_dir_ms=257",
        ),
        (
            "copy_between_volumes: 451 sources, dest=/Volumes/Team Share/Case 12 [Ref 3] Final Draft",
            "copy_between_volumes: 451 sources, dest=/Volumes/<volume>/<dir>",
        ),
        (
            "enrich: 7/7 dirs got sizes under /Volumes/Team Share/Case 12 [Ref 3] Final Draft",
            "enrich: 7/7 dirs got sizes under /Volumes/<volume>/<dir>",
        ),
        (
            "The share is mounted elsewhere (pane sent to /Volumes/Team Share/Case [A 1] Final QWERTY); following it",
            "The share is mounted elsewhere (pane sent to /Volumes/<volume>/<dir>); following it",
        ),
        (
            "conflict detected at /Volumes/Team Share/Plans/Report (final) (source_is_dir=true, dest_is_dir=true)",
            "conflict detected at /Volumes/<volume>/<dir>/<dir> (source_is_dir=true, dest_is_dir=true)",
        ),
        (
            "spawning copy /Users/alice/Work/Report (final) -> /Volumes/Team Share/Plans/Report (final)",
            "spawning copy $HOME/<dir>/<dir> -> /Volumes/<volume>/<dir>/<dir>",
        ),
        (
            "Cover: can't walk /srv/clients/Case [A 1] Tail: permission denied",
            "Cover: can't walk /<dir>/<dir>/<dir>: permission denied",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// A path that fills a quoted or backticked span ends at the closing quote, as at a `: ` seam,
/// so its lowercase last words are part of the name, not prose to give back. The frontend's
/// `destinationPath="…"` shipped them.
#[test]
fn a_path_filling_its_quotes_keeps_its_lowercase_last_words() {
    let cases = [
        (
            "Initial path resolution: destinationPath=\"/Volumes/Team Share/Case 12 [Ref 3] final draft\", currentVolumeId=root",
            "Initial path resolution: destinationPath=\"/Volumes/<volume>/<dir>\", currentVolumeId=root",
        ),
        (
            "Verifier: 6 diffs in `/Volumes/Team Share/appeal templates` (0 stale, 6 new dir)",
            "Verifier: 6 diffs in `/Volumes/<volume>/<dir>` (0 stale, 6 new dir)",
        ),
        (
            "skipping '/mnt/backup/tax returns' for now",
            "skipping '/mnt/<dir>/<dir>' for now",
        ),
        // A seam inside the quotes still ends the path.
        (
            "said \"/Volumes/Team Share/tax returns: failed\" once",
            "said \"/Volumes/<volume>/<dir>: failed\" once",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(r(input), expected, "input: {input:?}");
    }
}

/// The index verifier's diff samples: names keyed by kind, so none ships bare.
#[test]
fn verifier_diff_samples_are_keyed_names() {
    assert_eq!(
        r(
            r#"Verifier: 6 diffs in path="/Volumes/Team Share/Case [A 1]" (0 stale, 1 new files, 1 new dir, 0 modified) [samples: +dir="appeal templates", +file="notes v2.md", ~dir="__pycache__"]"#
        ),
        r#"Verifier: 6 diffs in path="/Volumes/<volume>/<dir>" (0 stale, 1 new files, 1 new dir, 0 modified) [samples: +dir="<dir>", +file="<file>.md", ~dir="<dir>"]"#
    );
}

#[test]
fn prose_redaction_is_idempotent() {
    let context = context();
    for input in [
        "listing_done path=/Volumes/Team Share/Case 12 [§ 3 AB] Final QWERTY entries=241",
        "conflict detected at /Volumes/Team Share/Plans/Report (final) (source_is_dir=true)",
        "Cover: can't walk /srv/clients/Acme Corp/plans.pdf: permission denied",
        "op 01a0 (Trash) failed: /Users/k/Desktop/Shot 1.jpeg: the Trash refused it: “Shot 1.jpeg”",
        "Installing update into bundle: /Applications/Cmdr.app",
    ] {
        let once = context.redact_line(input).into_owned();
        assert_eq!(context.redact_line(&once), once, "input: {input:?}");
        let bare = r(input);
        assert_eq!(r(&bare), bare, "input: {input:?}");
    }
}
