//! Test bundles built with `codesign` at test time. Nothing a test can build satisfies the release
//! requirement's team pin, so these prove the refusals, and the validity half against
//! [`fixture_requirement`].

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FIXTURE_ID: &str = "com.example.cmdr-updater-fixture";

/// A minimal `.app` at `app` with an executable, a resource, and an `Info.plist` naming
/// `version`, unsigned.
pub fn unsigned_bundle_at(app: &Path, version: &str) -> PathBuf {
    let contents = app.join("Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::create_dir_all(contents.join("Resources")).unwrap();
    // Any Mach-O will do, stripped of the Apple signature it ships with.
    let exe = contents.join("MacOS/Fixture");
    fs::copy("/usr/bin/true", &exe).unwrap();
    run_codesign(&[OsStr::new("--remove-signature"), exe.as_os_str()]);
    fs::write(contents.join("Resources/data.txt"), "sealed\n").unwrap();
    let mut info = plist::Dictionary::new();
    info.insert("CFBundleIdentifier".into(), FIXTURE_ID.into());
    info.insert("CFBundleExecutable".into(), "Fixture".into());
    info.insert("CFBundleShortVersionString".into(), version.into());
    plist::Value::Dictionary(info)
        .to_file_xml(contents.join("Info.plist"))
        .unwrap();
    app.to_path_buf()
}

/// The same bundle, ad-hoc signed: what someone holding only the updater key can produce.
pub fn ad_hoc_signed_bundle_at(app: &Path, version: &str) -> PathBuf {
    unsigned_bundle_at(app, version);
    run_codesign(&[
        OsStr::new("--force"),
        OsStr::new("--sign"),
        OsStr::new("-"),
        OsStr::new("--identifier"),
        OsStr::new(FIXTURE_ID),
        app.as_os_str(),
    ]);
    app.to_path_buf()
}

pub fn unsigned_fixture(dir: &Path) -> PathBuf {
    unsigned_bundle_at(&dir.join("Fixture.app"), "1.0.0")
}

pub fn ad_hoc_signed_fixture(dir: &Path) -> PathBuf {
    ad_hoc_signed_bundle_at(&dir.join("Fixture.app"), "1.0.0")
}

/// A requirement the ad-hoc fixture satisfies, so the tamper tests isolate signature validity
/// from the team pin.
pub fn fixture_requirement() -> String {
    format!("identifier \"{FIXTURE_ID}\"")
}

/// Packs `root`'s children into a `.tar.gz` at `archive`, the way the bundler does (`Cmdr.app/` at
/// the top).
pub fn tar_gz(root: &Path, archive: &Path) {
    let file = fs::File::create(archive).unwrap();
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(gz);
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        builder.append_dir_all(entry.file_name(), entry.path()).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();
}

fn run_codesign(args: &[&OsStr]) {
    let output = Command::new("codesign").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "codesign: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
