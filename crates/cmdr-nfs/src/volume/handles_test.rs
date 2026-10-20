use bytes::Bytes;

use super::*;
use crate::proto::nfs3::WccAttr;
use crate::proto::server::nfs3_wire::simple_attrs;

fn fh(n: u32) -> FileHandle {
    FileHandle(Bytes::from(n.to_be_bytes().to_vec()))
}

fn time(seconds: u32) -> NfsTime {
    NfsTime { seconds, nseconds: 0 }
}

#[test]
fn forgetting_a_folder_forgets_everything_under_it_and_nothing_beside_it() {
    let cache = HandleCache::new();
    for (index, path) in ["/a", "/a/b", "/a/b/c", "/ab", "/z"].iter().enumerate() {
        cache.put(path.to_string(), fh(index as u32), None);
    }
    cache.forget_subtree("/a");
    assert!(cache.get("/a").is_none() && cache.get("/a/b").is_none() && cache.get("/a/b/c").is_none());
    assert!(
        cache.get("/ab").is_some(),
        "a sibling sharing the prefix by bytes stays"
    );
    assert!(cache.get("/z").is_some());
    cache.forget_subtree("/");
    assert_eq!(cache.len(), 0);
}

#[test]
fn the_cache_stays_bounded_and_keeps_what_was_used_last() {
    let cache = HandleCache::new();
    cache.put("/keep".to_string(), fh(0), None);
    for index in 0..CACHE_CAP {
        cache.put(format!("/d{index}"), fh(index as u32), None);
        if index % 100 == 0 {
            cache.get("/keep");
        }
    }
    assert!(cache.len() <= CACHE_CAP);
    assert!(cache.get("/keep").is_some(), "a hot entry survives an eviction");
}

#[test]
fn a_listing_never_overwrites_a_stamped_lookup() {
    let cache = HandleCache::new();
    let stamp = (time(5), time(5));
    cache.put("/a".to_string(), fh(1), Some(stamp));
    cache.offer("/a".to_string(), fh(1));
    assert_eq!(cache.get("/a").unwrap().1, Some(stamp));
}

#[test]
fn our_own_change_moves_only_the_stamps_that_saw_the_folder_as_it_was() {
    let cache = HandleCache::new();
    let was = (time(5), time(5));
    cache.put("/d/a".to_string(), fh(1), Some(was));
    cache.put("/d/b".to_string(), fh(2), Some((time(4), time(4))));
    cache.put("/d/a/deeper".to_string(), fh(3), Some(was));
    let mut after = simple_attrs(FileType::Directory, 9, 0);
    after.mtime = time(6);
    after.ctime = time(6);
    cache.note_dir_changed(
        "/d",
        &WccData {
            before: Some(WccAttr {
                size: 0,
                mtime: time(5),
                ctime: time(5),
            }),
            after: Some(after),
        },
    );
    assert_eq!(cache.get("/d/a").unwrap().1, Some((time(6), time(6))));
    assert_eq!(
        cache.get("/d/b").unwrap().1,
        Some((time(4), time(4))),
        "it saw another change too"
    );
    assert_eq!(cache.get("/d/a/deeper").unwrap().1, Some(was), "not a child of `/d`");
}
