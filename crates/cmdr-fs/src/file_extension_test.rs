use super::*;
use crate::archive_format::{ArchiveFormat, format_for_name};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    package_extensions: Vec<String>,
    compound_extensions: Vec<String>,
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    is_directory: bool,
    extension: String,
}

/// The rule's spec, shared with the frontend's `file-extension.test.ts`, which
/// runs the same cases against its mirror. Both suites reading ONE file is what
/// keeps the two languages agreeing.
fn spec() -> Spec {
    serde_json::from_str(include_str!("file_extension_cases.json")).expect("the shared cases parse")
}

#[test]
fn every_shared_case_holds() {
    for case in spec().cases {
        let got = extension_of(&case.name, case.is_directory).unwrap_or("");
        assert_eq!(
            got, case.extension,
            "{:?} (folder: {}) should have extension {:?}",
            case.name, case.is_directory, case.extension
        );
    }
}

#[test]
fn the_package_list_is_the_shared_one() {
    let mut ours: Vec<&str> = PACKAGE_EXTENSIONS.to_vec();
    ours.sort_unstable();
    let mut shared = spec().package_extensions;
    shared.sort_unstable();
    assert_eq!(ours, shared);
}

#[test]
fn the_compound_list_is_the_shared_one() {
    assert_eq!(COMPOUND_EXTENSIONS.to_vec(), spec().compound_extensions);
}

/// The compounds are exactly the compressed tars `archive_format` names with a
/// dotted label, so an archive the app browses and the extension the rename
/// field selects can't disagree on where the name ends.
#[test]
fn the_compounds_are_the_dotted_archive_labels() {
    for compound in COMPOUND_EXTENSIONS {
        let format = format_for_name(&format!("a.{compound}")).expect("a compound names an archive");
        assert!(matches!(format, ArchiveFormat::Tar(_)), "{compound} is a tar");
        assert_eq!(format.label(), *compound);
    }
}

#[test]
fn name_without_extension_strips_the_dot_too() {
    assert_eq!(name_without_extension("report.pdf", false), "report");
    assert_eq!(name_without_extension("src.tar.gz", false), "src");
    assert_eq!(
        name_without_extension("Smlouva v. 2 final", false),
        "Smlouva v. 2 final"
    );
    assert_eq!(name_without_extension("release.v1.2", true), "release.v1.2");
    assert_eq!(name_without_extension("Cmdr.app", true), "Cmdr");
}

#[test]
fn a_multibyte_tail_never_panics() {
    assert_eq!(extension_of("ünïcödé.tär.gz", false), Some("gz"));
    assert_eq!(extension_of("日本語", false), None);
    assert_eq!(extension_of("é.gz", false), Some("gz"));
}
