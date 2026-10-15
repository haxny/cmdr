//! Apple code-signature check on a staged update, run before anything is written into the bundle.
//!
//! The minisign signature proves an archive came from whoever holds the updater key. This proves
//! the bundle inside it was signed by Cmdr's Developer ID, which is a separate key held by Apple's
//! chain. Requiring both means a leaked updater key alone can't push a build to every install.

use std::path::Path;

/// Why a staged bundle's Apple code signature doesn't make it a Cmdr release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CodeSignatureRefusal {
    /// The bundle carries no code signature at all.
    Unsigned,
    /// The signature is intact but isn't Cmdr's (ad-hoc, another team, another app).
    NotOurs,
    /// The signature doesn't hold: a sealed file was changed, added, or removed, or the signature
    /// itself is malformed. `status` is the Security framework's `OSStatus`.
    Invalid { status: i32 },
}

impl std::fmt::Display for CodeSignatureRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsigned => f.write_str("the bundle isn't code-signed"),
            Self::NotOurs => f.write_str("the bundle isn't signed by Cmdr's Developer ID"),
            Self::Invalid { status } => write!(f, "the bundle's code signature doesn't hold (OSStatus {status})"),
        }
    }
}

/// What a Cmdr release's code signature must satisfy. It's byte for byte the designated requirement
/// `codesign` gives every release (`codesign -d -r- Cmdr.app`), so it can't be stricter than what a
/// real release carries:
///
/// - `identifier "com.veszelovszki.cmdr"`: the bundle is Cmdr, not another program signed by the
///   same team. TCC keys the Full Disk Access grant on this same requirement.
/// - `anchor apple generic`: the chain ends at Apple's root, so a self-made CA can't satisfy it.
/// - `certificate 1[field.1.2.840.113635.100.6.2.6]`: the intermediate is Apple's Developer ID CA.
/// - `certificate leaf[field.1.2.840.113635.100.6.1.13]`: the leaf is a Developer ID Application
///   certificate (not Mac App Store, not development).
/// - `certificate leaf[subject.OU] = "83H6YAQMNP"`: and it was issued to our team (Rymdskottkarra
///   AB). This is the pin: Apple issues Developer ID certificates to anyone who pays, so without it
///   any developer's signed build would pass.
///
/// Renewing the Developer ID certificate keeps the team ID, so a renewal doesn't break updates.
/// Changing the bundle identifier or the team would, along with every Full Disk Access grant.
const RELEASE_REQUIREMENT: &str = concat!(
    r#"identifier "com.veszelovszki.cmdr""#,
    " and anchor apple generic",
    " and certificate 1[field.1.2.840.113635.100.6.2.6]",
    " and certificate leaf[field.1.2.840.113635.100.6.1.13]",
    r#" and certificate leaf[subject.OU] = "83H6YAQMNP""#,
);

// `Security/CSCommon.h`; `security-framework-sys` doesn't export the code-signing statuses.
const ERR_SEC_CS_UNSIGNED: i32 = -67062;
const ERR_SEC_CS_REQ_FAILED: i32 = -67050;
const ERR_SEC_CS_STATIC_CODE_NOT_FOUND: i32 = -67068;

/// Refuses `bundle` unless it carries a valid Apple code signature from Cmdr's Developer ID.
///
/// Runs on every install, with nothing to switch it off: see `DETAILS.md` § Code signature.
pub(super) fn verify_release_bundle(bundle: &Path) -> Result<(), CodeSignatureRefusal> {
    verify_bundle_against(bundle, RELEASE_REQUIREMENT)
}

/// The check with the requirement injected, so tests can prove the validity half on an ad-hoc
/// signed fixture (nothing a test can build satisfies the team pin).
fn verify_bundle_against(bundle: &Path, requirement: &str) -> Result<(), CodeSignatureRefusal> {
    use core_foundation::url::CFURL;
    use security_framework::os::macos::code_signing::{Flags, SecRequirement, SecStaticCode};

    let invalid = |e: security_framework::base::Error| match e.code() {
        ERR_SEC_CS_UNSIGNED => CodeSignatureRefusal::Unsigned,
        ERR_SEC_CS_REQ_FAILED => CodeSignatureRefusal::NotOurs,
        status => CodeSignatureRefusal::Invalid { status },
    };

    let url = CFURL::from_path(bundle, true).ok_or(CodeSignatureRefusal::Invalid {
        status: ERR_SEC_CS_STATIC_CODE_NOT_FOUND,
    })?;
    let requirement = requirement.parse::<SecRequirement>().map_err(invalid)?;
    let code = SecStaticCode::from_path(&url, Flags::NONE).map_err(invalid)?;
    // The strictest validation `codesign --verify --strict --deep` does: every architecture, every
    // sealed resource (changed, added, or removed files all fail), nested code recursively,
    // `kSecCSStrictValidate`'s extra structural checks, and no symlink pointing out of the bundle.
    let flags =
        Flags::CHECK_ALL_ARCHITECTURES | Flags::STRICT_VALIDATE | Flags::CHECK_NESTED_CODE | Flags::RESTRICT_SYMLINKS;
    code.check_validity(flags, &requirement).map_err(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    /// A minimal `.app` with an executable, a resource, and an `Info.plist`, unsigned.
    fn unsigned_fixture(dir: &TestDir) -> PathBuf {
        let app = dir.join("Fixture.app");
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
        plist::Value::Dictionary(info)
            .to_file_xml(contents.join("Info.plist"))
            .unwrap();
        app
    }

    const FIXTURE_ID: &str = "com.example.cmdr-updater-fixture";

    /// The same fixture, ad-hoc signed: what someone holding only the updater key can produce.
    fn ad_hoc_signed_fixture(dir: &TestDir) -> PathBuf {
        let app = unsigned_fixture(dir);
        run_codesign(&[
            OsStr::new("--force"),
            OsStr::new("--sign"),
            OsStr::new("-"),
            OsStr::new("--identifier"),
            OsStr::new(FIXTURE_ID),
            app.as_os_str(),
        ]);
        app
    }

    fn run_codesign(args: &[&OsStr]) {
        let output = Command::new("codesign").args(args).output().unwrap();
        assert!(
            output.status.success(),
            "codesign: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A requirement the ad-hoc fixture satisfies, so the tamper tests isolate signature validity
    /// from the team pin.
    fn fixture_requirement() -> String {
        format!("identifier \"{FIXTURE_ID}\"")
    }

    #[test]
    fn the_release_requirement_compiles() {
        use security_framework::os::macos::code_signing::SecRequirement;
        assert!(RELEASE_REQUIREMENT.parse::<SecRequirement>().is_ok());
    }

    /// ❗ The core guarantee: whoever holds only the updater key can sign an archive, but can only
    /// put an ad-hoc (or their own team's) code signature inside it.
    #[test]
    fn an_ad_hoc_signed_bundle_is_refused() {
        let dir = TestDir::new("codesig_adhoc");
        let app = ad_hoc_signed_fixture(&dir);
        assert_eq!(verify_release_bundle(&app), Err(CodeSignatureRefusal::NotOurs));
    }

    #[test]
    fn an_unsigned_bundle_is_refused() {
        let dir = TestDir::new("codesig_unsigned");
        let app = unsigned_fixture(&dir);
        assert_eq!(verify_release_bundle(&app), Err(CodeSignatureRefusal::Unsigned));
    }

    /// Validly signed by Apple itself still isn't Cmdr: the team pin, not "any trusted chain", is
    /// what decides.
    #[test]
    fn an_apple_signed_app_that_isnt_cmdr_is_refused() {
        let calculator = Path::new("/System/Applications/Calculator.app");
        assert_eq!(verify_release_bundle(calculator), Err(CodeSignatureRefusal::NotOurs));
    }

    /// The mechanics with a requirement the fixture meets: an intact bundle passes.
    #[test]
    fn an_intact_bundle_meeting_the_requirement_passes() {
        let dir = TestDir::new("codesig_intact");
        let app = ad_hoc_signed_fixture(&dir);
        assert_eq!(verify_bundle_against(&app, &fixture_requirement()), Ok(()));
    }

    #[test]
    fn a_changed_resource_is_refused() {
        let dir = TestDir::new("codesig_resource");
        let app = ad_hoc_signed_fixture(&dir);
        fs::write(app.join("Contents/Resources/data.txt"), "tampered\n").unwrap();
        assert!(matches!(
            verify_bundle_against(&app, &fixture_requirement()),
            Err(CodeSignatureRefusal::Invalid { .. })
        ));
    }

    #[test]
    fn an_added_resource_is_refused() {
        let dir = TestDir::new("codesig_added");
        let app = ad_hoc_signed_fixture(&dir);
        fs::write(app.join("Contents/Resources/extra.txt"), "smuggled\n").unwrap();
        assert!(matches!(
            verify_bundle_against(&app, &fixture_requirement()),
            Err(CodeSignatureRefusal::Invalid { .. })
        ));
    }

    #[test]
    fn a_changed_executable_is_refused() {
        let dir = TestDir::new("codesig_exe");
        let app = ad_hoc_signed_fixture(&dir);
        let exe = app.join("Contents/MacOS/Fixture");
        let mut bytes = fs::read(&exe).unwrap();
        // Flip a byte inside the first page, which the code directory hashes.
        bytes[0x200] ^= 0xFF;
        fs::write(&exe, bytes).unwrap();
        assert!(matches!(
            verify_bundle_against(&app, &fixture_requirement()),
            Err(CodeSignatureRefusal::Invalid { .. })
        ));
    }

    #[test]
    fn a_missing_bundle_is_refused() {
        let dir = TestDir::new("codesig_missing");
        assert!(verify_release_bundle(&dir.join("Nothing.app")).is_err());
    }
}
