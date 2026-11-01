//! Scan → plan: folders the scan leaves out of `entries` still head rows, so a
//! run converges instead of finding its landing taken every time.

use std::path::Path;

use cmdr_fs::volume::{InMemoryVolume, Volume};

use crate::*;

async fn volume(files: &[&str], dirs: &[&str]) -> InMemoryVolume {
    let volume = InMemoryVolume::new("t");
    for dir in dirs {
        volume.create_directory(Path::new(dir)).await.unwrap();
    }
    for path in files {
        volume.create_file(Path::new(path), b"x").await.unwrap();
        volume.set_modified_at(Path::new(path), Some(1_700_000_000));
    }
    volume
}

/// Empty folders off, hidden off, `.DS_Store` junk: the defaults that drop a
/// folder holding nothing admitted.
fn options() -> ScanOptions {
    ScanOptions {
        subfolders: true,
        include_hidden: false,
        empty_folders: false,
        filter: MaskFilter::parse("", &[".DS_Store".to_string()]).unwrap(),
        name_rule: NameRule::FoldCase,
        only: None,
    }
}

async fn snap(side: Side, volume: &InMemoryVolume) -> Snapshot {
    scan(side, volume, Path::new("/"), &options(), &SyncControl::new(), &|_| {})
        .await
        .unwrap()
}

async fn plan_of(left: &InMemoryVolume, right: &InMemoryVolume, mode: Mode) -> Plan {
    let (l, r) = (snap(Side::Left, left).await, snap(Side::Right, right).await);
    plan(
        &l,
        &r,
        None,
        &PlanOptions {
            mode,
            rules: CompareRules::QUICK,
        },
    )
}

fn key(parts: &[&str]) -> EntryKey {
    EntryKey::Name(parts.iter().map(|p| p.to_string()).collect())
}

fn row<'a>(plan: &'a Plan, parts: &[&str]) -> Option<&'a PlanItem> {
    plan.items.iter().find(|item| item.key == key(parts))
}

async fn assert_children_land_inside(right: InMemoryVolume) {
    let left = volume(&["/d/a.txt"], &["/d"]).await;
    let plan = plan_of(&left, &right, Mode::Update).await;
    assert!(
        row(&plan, &["d"]).is_none(),
        "the folder is not copied whole over the existing one"
    );
    let child = row(&plan, &["d", "a.txt"]).expect("the child is its own row");
    assert_eq!(child.action, Action::CopyRight);
    let head = &plan.heads[&key(&["d"])];
    assert_eq!(
        head.right.as_deref(),
        Some("d"),
        "the copy lands in the right's own folder"
    );
}

#[tokio::test]
async fn a_target_folder_holding_only_junk_heads_its_children() {
    let right = volume(&["/d/.DS_Store"], &["/d"]).await;
    let snap = snap(Side::Right, &right).await;
    assert!(snap.entries.is_empty());
    assert_eq!(snap.present_folders.get(&key(&["d"])).map(String::as_str), Some("d"));
    assert_children_land_inside(right).await;
}

#[tokio::test]
async fn a_target_folder_holding_only_hidden_files_heads_its_children() {
    let right = volume(&["/d/.hidden"], &["/d"]).await;
    assert_children_land_inside(right).await;
}

#[tokio::test]
async fn a_nested_junk_only_folder_is_present_at_every_level() {
    let left = volume(&["/d/e/a.txt"], &["/d", "/d/e"]).await;
    let right = volume(&["/d/e/.DS_Store"], &["/d", "/d/e"]).await;
    let plan = plan_of(&left, &right, Mode::Mirror).await;
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].key, key(&["d", "e", "a.txt"]));
    assert_eq!(plan.heads[&key(&["d", "e"])].right.as_deref(), Some("d/e"));
}

#[tokio::test]
async fn a_file_facing_a_junk_only_folder_is_a_type_clash() {
    let left = volume(&["/x"], &[]).await;
    let right = volume(&["/x/.DS_Store"], &["/x"]).await;
    let plan = plan_of(&left, &right, Mode::Mirror).await;
    let item = row(&plan, &["x"]).unwrap();
    assert_eq!(item.verdict.category, Category::TypeClash);
    assert_eq!(item.action, Action::Skip);
}
