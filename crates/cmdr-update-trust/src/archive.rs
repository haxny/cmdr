//! Update archives: extracting one the way the installer does, and vetting one the way the release
//! gate does.

use std::fs;
use std::path::Path;

use crate::code_signature::{CodeSignatureRefusal, verify_release_bundle};

/// Why an update archive isn't a Cmdr release the updater would install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveRefusal {
    /// The archive couldn't be opened or unpacked.
    Unreadable {
        /// What went wrong, for logs.
        detail: String,
    },
    /// The archive unpacked, but holds no `Cmdr.app/` at its top.
    NoBundle,
    /// The bundle's Apple code signature isn't Cmdr's Developer ID (the check the app runs).
    CodeSignature(CodeSignatureRefusal),
    /// The bundle's `Info.plist` names no readable version.
    VersionUnreadable {
        /// What went wrong, for logs.
        detail: String,
    },
    /// The bundle names another version than the release it ships in.
    WrongVersion {
        /// The version the bundle's `Info.plist` names.
        found: String,
        /// The release's version.
        expected: String,
    },
}

impl std::fmt::Display for ArchiveRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable { detail } => write!(f, "the archive doesn't unpack: {detail}"),
            Self::NoBundle => f.write_str("the archive holds no Cmdr.app at its top"),
            Self::CodeSignature(refusal) => write!(f, "{refusal}, so every install would refuse it"),
            Self::VersionUnreadable { detail } => write!(f, "the bundle's version can't be read: {detail}"),
            Self::WrongVersion { found, expected } => {
                write!(f, "the bundle is version {found}, but this is the {expected} release")
            }
        }
    }
}

/// Extracts a `.tar.gz` tarball into `dest_dir` with the `tar` crate, the one extraction the
/// installer uses. It drops xattrs and sets its own modes, which is why the release gate extracts
/// with it too: a bundle that verifies after the system `tar` might not after this.
pub fn extract_tarball(tarball_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(tarball_path).map_err(|e| format!("Couldn't open tarball: {e}"))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive
        .unpack(dest_dir)
        .map_err(|e| format!("Couldn't extract tarball: {e}"))
}

/// The version a bundle's `Info.plist` names (`CFBundleShortVersionString`). The updater's
/// rollback check and the release gate both read it here.
pub fn bundle_version(app: &Path) -> Result<String, String> {
    let info_path = app.join("Contents/Info.plist");
    let info = plist::Value::from_file(&info_path).map_err(|e| format!("Couldn't read the Info.plist: {e}"))?;
    info.as_dictionary()
        .and_then(|d| d.get("CFBundleShortVersionString"))
        .and_then(plist::Value::as_string)
        .map(str::to_owned)
        .ok_or_else(|| "The Info.plist names no version".to_string())
}

/// Refuses `tarball` unless it unpacks, with the installer's own extraction, to a `Cmdr.app`
/// code-signed by Cmdr's Developer ID that names `expected_version`.
pub fn vet_update_archive(tarball: &Path, expected_version: &str) -> Result<(), ArchiveRefusal> {
    vet_with(tarball, expected_version, &verify_release_bundle)
}

/// [`vet_update_archive`] with the code-signature check injected, so tests can reach the version
/// check with a fixture nothing can sign as Cmdr.
fn vet_with(
    tarball: &Path,
    expected_version: &str,
    code_signature: &dyn Fn(&Path) -> Result<(), CodeSignatureRefusal>,
) -> Result<(), ArchiveRefusal> {
    let unpacked = tempfile::tempdir().map_err(|e| ArchiveRefusal::Unreadable {
        detail: format!("Couldn't make a dir to unpack into: {e}"),
    })?;
    extract_tarball(tarball, unpacked.path()).map_err(|detail| ArchiveRefusal::Unreadable { detail })?;
    let app = unpacked.path().join("Cmdr.app");
    if !app.is_dir() {
        return Err(ArchiveRefusal::NoBundle);
    }
    // First, as in the installer: until it holds, the `Info.plist` is only what the signer wrote.
    code_signature(&app).map_err(ArchiveRefusal::CodeSignature)?;
    let found = bundle_version(&app).map_err(|detail| ArchiveRefusal::VersionUnreadable { detail })?;
    if found != expected_version {
        return Err(ArchiveRefusal::WrongVersion {
            found,
            expected: expected_version.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{ad_hoc_signed_bundle_at, tar_gz, unsigned_bundle_at};
    use std::path::PathBuf;

    /// An archive of a `Cmdr.app` built by `build`, the way the bundler packs one.
    fn archive_of(dir: &Path, build: impl Fn(&Path) -> PathBuf) -> PathBuf {
        let root = dir.join("root");
        build(&root.join("Cmdr.app"));
        let archive = dir.join("Cmdr.app.tar.gz");
        tar_gz(&root, &archive);
        archive
    }

    fn trusted(_: &Path) -> Result<(), CodeSignatureRefusal> {
        Ok(())
    }

    /// ❗ The gate's reason to exist: an archive the updater key could sign, but whose bundle isn't
    /// Cmdr's Developer ID build, never gets signed, since every install would refuse it.
    #[test]
    fn an_ad_hoc_signed_bundle_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = archive_of(dir.path(), |app| ad_hoc_signed_bundle_at(app, "1.0.0"));
        assert_eq!(
            vet_update_archive(&archive, "1.0.0"),
            Err(ArchiveRefusal::CodeSignature(CodeSignatureRefusal::NotOurs))
        );
    }

    #[test]
    fn an_unsigned_bundle_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = archive_of(dir.path(), |app| unsigned_bundle_at(app, "1.0.0"));
        assert_eq!(
            vet_update_archive(&archive, "1.0.0"),
            Err(ArchiveRefusal::CodeSignature(CodeSignatureRefusal::Unsigned))
        );
    }

    #[test]
    fn a_bundle_naming_another_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = archive_of(dir.path(), |app| unsigned_bundle_at(app, "1.0.0"));
        assert_eq!(
            vet_with(&archive, "1.0.1", &trusted),
            Err(ArchiveRefusal::WrongVersion {
                found: "1.0.0".into(),
                expected: "1.0.1".into()
            })
        );
    }

    #[test]
    fn a_signed_bundle_naming_the_release_version_passes() {
        let dir = tempfile::tempdir().unwrap();
        let archive = archive_of(dir.path(), |app| unsigned_bundle_at(app, "1.0.0"));
        assert_eq!(vet_with(&archive, "1.0.0", &trusted), Ok(()));
    }

    /// The code signature goes first: until it holds, the `Info.plist` is only what the signer wrote.
    #[test]
    fn the_code_signature_is_checked_before_the_version() {
        let dir = tempfile::tempdir().unwrap();
        let archive = archive_of(dir.path(), |app| ad_hoc_signed_bundle_at(app, "9.9.9"));
        assert_eq!(
            vet_update_archive(&archive, "1.0.0"),
            Err(ArchiveRefusal::CodeSignature(CodeSignatureRefusal::NotOurs))
        );
    }

    #[test]
    fn an_archive_without_cmdr_app_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        unsigned_bundle_at(&root.join("Other.app"), "1.0.0");
        let archive = dir.path().join("Other.app.tar.gz");
        tar_gz(&root, &archive);
        assert_eq!(vet_with(&archive, "1.0.0", &trusted), Err(ArchiveRefusal::NoBundle));
    }

    #[test]
    fn a_file_that_isnt_an_archive_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("Cmdr.app.tar.gz");
        fs::write(&archive, "not a tarball").unwrap();
        assert!(matches!(
            vet_with(&archive, "1.0.0", &trusted),
            Err(ArchiveRefusal::Unreadable { .. })
        ));
    }

    /// The check against a real published release, through the same extraction the installer
    /// uses: the genuine archive passes, an ad-hoc re-signed bundle and one with a changed byte
    /// don't. Needs the release archive, so it's opt-in; recipe in `DETAILS.md`.
    #[test]
    #[ignore = "needs a downloaded release archive in CMDR_UPDATER_RELEASE_TARBALL"]
    fn a_published_release_passes_and_its_doctored_copies_dont() {
        use std::process::Command;
        let tarball =
            PathBuf::from(std::env::var_os("CMDR_UPDATER_RELEASE_TARBALL").expect("set CMDR_UPDATER_RELEASE_TARBALL"));
        let extract = || {
            let dir = tempfile::tempdir().unwrap();
            extract_tarball(&tarball, dir.path()).unwrap();
            let app = dir.path().join("Cmdr.app");
            (dir, app)
        };

        let (_genuine_dir, genuine) = extract();
        assert_eq!(verify_release_bundle(&genuine), Ok(()));
        let version = bundle_version(&genuine).unwrap();
        assert_eq!(vet_update_archive(&tarball, &version), Ok(()));

        let (_ad_hoc_dir, ad_hoc) = extract();
        let output = Command::new("codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&ad_hoc)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "codesign: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(verify_release_bundle(&ad_hoc), Err(CodeSignatureRefusal::NotOurs));

        let (_tampered_dir, tampered) = extract();
        let mut info = fs::read(tampered.join("Contents/Info.plist")).unwrap();
        info.extend_from_slice(b"\n");
        fs::write(tampered.join("Contents/Info.plist"), info).unwrap();
        assert!(matches!(
            verify_release_bundle(&tampered),
            Err(CodeSignatureRefusal::Invalid { .. })
        ));
    }
}
