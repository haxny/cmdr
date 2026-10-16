//! The `tauri-plugin-store` save that `settings.json`, `shortcuts.json`, and `license.json` go
//! through is a temp file renamed over the target, never a truncate-then-write. Cmdr patches the
//! plugin for it (`vendor/tauri-plugin-store/`), so these tests run the real plugin on a mock app:
//! they fail the moment the patch stops applying.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;

use tauri::test::MockRuntime;
use tauri_plugin_store::{Store, StoreExt};

use crate::test_support::TestDir;

fn app() -> tauri::App<MockRuntime> {
    let app = tauri::test::mock_app();
    app.handle()
        .plugin(tauri_plugin_store::Builder::default().build())
        .expect("the store plugin registers on a mock app");
    app
}

fn store_at(app: &tauri::App<MockRuntime>, path: &Path) -> Arc<Store<MockRuntime>> {
    app.store_builder(path)
        .disable_auto_save()
        .build()
        .expect("a store at an absolute path")
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read the store's dir")
        .map(|entry| entry.expect("a dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_unstable();
    names
}

/// A rename swaps in a new file (a new inode) and leaves nothing beside it; a truncate-then-write
/// keeps the inode, which is exactly the window where a crash leaves a half-written file.
#[test]
fn a_save_renames_a_finished_file_over_the_old_one() {
    let dir = TestDir::new("plugin_store_rename");
    let path = dir.join("settings.json");
    let app = app();
    let store = store_at(&app, &path);

    store.set("listing.showHiddenFiles", true);
    store.save().expect("the first save");
    let first_inode = fs::metadata(&path).expect("the file exists").ino();

    store.set("listing.showHiddenFiles", false);
    store.save().expect("the second save");

    assert_ne!(fs::metadata(&path).expect("the file exists").ino(), first_inode);
    assert_eq!(
        names_in(&dir),
        vec!["settings.json".to_string()],
        "no temp file left behind"
    );
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).expect("read back")).expect("valid JSON");
    assert_eq!(saved["listing.showHiddenFiles"], serde_json::json!(false));
}

/// A save that can't finish (here a read-only folder, the same shape as a full disk) reports
/// itself to the caller and leaves the old file byte for byte.
#[test]
fn a_save_that_cant_finish_leaves_the_old_file_and_says_so() {
    // Root ignores the folder's permissions (the Linux Docker lane runs as root).
    // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail — it just
    // returns the caller's effective uid. We compare the returned integer to 0 to detect root.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = TestDir::new("plugin_store_readonly");
    let path = dir.join("settings.json");
    let app = app();
    let store = store_at(&app, &path);

    store.set("analytics.enabled", false);
    store.save().expect("the first save");
    let before = fs::read(&path).expect("read the saved file");

    fs::set_permissions(&*dir, fs::Permissions::from_mode(0o555)).expect("make the folder read-only");
    store.set("analytics.enabled", true);
    let outcome = store.save();
    fs::set_permissions(&*dir, fs::Permissions::from_mode(0o755)).expect("make the folder writable again");

    assert!(outcome.is_err(), "the caller hears that the save didn't land");
    assert_eq!(fs::read(&path).expect("read the file again"), before);
    assert_eq!(names_in(&dir), vec!["settings.json".to_string()]);
}

/// A rename that fails after the temp file was written (here the target is a folder) cleans the
/// temp file up, so a stray copy can't pile up beside the store.
#[test]
fn a_save_whose_rename_fails_removes_its_temp_file() {
    let dir = TestDir::new("plugin_store_rename_fails");
    let path = dir.join("settings.json");
    fs::create_dir(&path).expect("a folder where the file should be");
    fs::write(path.join("keep"), b"x").expect("a non-empty folder can't be renamed over");
    let app = app();
    let store = store_at(&app, &path);

    store.set("analytics.enabled", false);
    assert!(store.save().is_err());
    assert_eq!(names_in(&dir), vec!["settings.json".to_string()]);
}
