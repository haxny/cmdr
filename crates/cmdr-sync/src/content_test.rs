use std::path::Path;

use cmdr_fs::volume::{BackendKind, InMemoryVolume, Volume};

use crate::*;

async fn pair(left: &[u8], right: &[u8]) -> (InMemoryVolume, InMemoryVolume) {
    let l = InMemoryVolume::new("l");
    let r = InMemoryVolume::new("r");
    l.create_file(Path::new("/f"), left).await.unwrap();
    r.create_file(Path::new("/f"), right).await.unwrap();
    (l, r)
}

fn job(item: u32, size: u64) -> ContentJob {
    ContentJob {
        item,
        left: "/f".into(),
        right: "/f".into(),
        size,
    }
}

async fn one(l: &InMemoryVolume, r: &InMemoryVolume, size: u64) -> ContentState {
    let out = compare_contents(l, r, vec![job(0, size)], 1, &SyncControl::new(), &|_| {}).await;
    assert_eq!(out.len(), 1);
    out[0].1
}

#[tokio::test]
async fn same_bytes_are_same_and_one_flipped_byte_differs() {
    let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    let (l, r) = pair(&data, &data).await;
    assert_eq!(one(&l, &r, data.len() as u64).await, ContentState::Same);
    let mut flipped = data.clone();
    *flipped.last_mut().unwrap() ^= 1;
    let (l, r) = pair(&data, &flipped).await;
    assert_eq!(one(&l, &r, data.len() as u64).await, ContentState::Differs);
}

#[tokio::test]
async fn empty_files_are_same_and_a_shorter_side_differs() {
    let (l, r) = pair(b"", b"").await;
    assert_eq!(one(&l, &r, 0).await, ContentState::Same);
    let (l, r) = pair(b"abc", b"abcd").await;
    assert_eq!(one(&l, &r, 3).await, ContentState::Differs);
}

#[tokio::test]
async fn unreadable_side_is_unreadable() {
    let l = InMemoryVolume::new("l");
    let r = InMemoryVolume::new("r");
    l.create_file(Path::new("/f"), b"abc").await.unwrap();
    assert_eq!(one(&l, &r, 3).await, ContentState::Unreadable);
}

#[tokio::test]
async fn cancel_leaves_unstarted_jobs_pending() {
    let (l, r) = pair(b"abc", b"abc").await;
    let control = SyncControl::new();
    control.cancel();
    let out = compare_contents(&l, &r, vec![job(4, 3), job(9, 3)], 2, &control, &|_| {}).await;
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|(_, s)| *s == ContentState::Pending));
    let mut items: Vec<_> = out.iter().map(|(i, _)| *i).collect();
    items.sort();
    assert_eq!(items, [4, 9]);
}

#[tokio::test]
async fn many_jobs_finish_with_progress_reaching_the_total() {
    let (l, r) = pair(b"hello", b"hello").await;
    let last = std::sync::Mutex::new(None);
    let jobs = (0..6).map(|i| job(i, 5)).collect();
    let out = compare_contents(&l, &r, jobs, 4, &SyncControl::new(), &|p| {
        *last.lock().unwrap() = Some(p)
    })
    .await;
    assert!(out.iter().all(|(_, s)| *s == ContentState::Same));
    let p = last.lock().unwrap().take().unwrap();
    assert_eq!(
        (p.files_done, p.files_total, p.bytes_done, p.bytes_total),
        (6, 6, 30, 30)
    );
}

#[test]
fn one_worker_when_both_sides_are_local_else_four() {
    let local = InMemoryVolume::new("a");
    let other = InMemoryVolume::new("b").with_backend_kind(BackendKind::Smb);
    assert_eq!(content_workers(&local, &InMemoryVolume::new("c")), 1);
    assert_eq!(content_workers(&local, &other), 4);
}
