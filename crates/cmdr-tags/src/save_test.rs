//! `save_safely`: the original survives every failure, and a successful write keeps the
//! file's permissions, extended attributes, and links.

use std::fs;
use std::path::PathBuf;

use crate::model::TagError;
use crate::save::save_safely;
use crate::test_files::write_untagged_mp3;
use crate::write::write_tags;
use crate::write_test::{MUSIC, every_field, listing, read, strings, write};

#[test]
fn failed_save_leaves_original_and_no_temp() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());

    let result = save_safely(&path, |out| {
        out.write_all(b"half a tag")?;
        Err(TagError::Write("stopped partway".to_owned()))
    });

    assert!(matches!(result, Err(TagError::Write(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

#[cfg(unix)]
#[test]
fn failed_write_leaves_original_and_no_temp() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let result = write_tags(&path, &every_field(), &MUSIC);

    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(result, Err(TagError::Write(_) | TagError::Io(..))),
        "{result:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

#[cfg(unix)]
#[test]
fn write_keeps_permissions_and_leaves_no_temp() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

    write(&path, &every_field(), &MUSIC);

    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(listing(dir.path()), strings(&["a.mp3"]));
}

#[cfg(unix)]
#[test]
fn read_only_file_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    let before = fs::read(&path).unwrap();

    let result = write_tags(&path, &every_field(), &MUSIC);

    assert!(
        matches!(result, Err(TagError::Io(std::io::ErrorKind::PermissionDenied, _))),
        "{result:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn write_through_a_symlink_keeps_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let target = write_untagged_mp3(dir.path(), "a.mp3");
    let link: PathBuf = dir.path().join("link.mp3");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    write(&link, &every_field(), &MUSIC);

    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(read(&target), every_field());
}

#[cfg(target_os = "macos")]
#[test]
fn write_keeps_extended_attributes() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let status = Command::new("xattr")
        .args(["-w", "com.cmdr.test", "hodnota"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());

    write(&path, &every_field(), &MUSIC);

    let out = Command::new("xattr")
        .args(["-p", "com.cmdr.test"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hodnota");
}

/// lofty has panicked mid-save (an MP4 ilst it can't encode); the panic must not escape the
/// write or leave the temp file behind.
#[test]
fn panicking_save_leaves_original_and_no_temp() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());

    let result = save_safely(&path, |_| panic!("the tag library gave up"));

    assert!(matches!(result, Err(TagError::Write(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}
