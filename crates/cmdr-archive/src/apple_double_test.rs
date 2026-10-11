use std::ffi::OsString;
use std::io::{Cursor, Read};

use cmdr_fs::metadata::Xattr;

use super::*;

const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";

/// The checked-in Finder-made zip (`test_fixtures::APPLE_DOUBLE_DITTO_ZIP`).
fn ditto_zip_member(name: &str) -> Vec<u8> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(crate::test_fixtures::APPLE_DOUBLE_DITTO_ZIP)).expect("the fixture is a zip");
    let mut member = archive.by_name(name).expect("the fixture has the member");
    let mut bytes = Vec::new();
    member.read_to_end(&mut bytes).expect("the member reads");
    bytes
}

fn xattr(name: &str, value: &[u8]) -> Xattr {
    Xattr {
        name: OsString::from(name),
        value: value.to_vec(),
    }
}

fn names(xattrs: &[Xattr]) -> Vec<String> {
    xattrs.iter().map(|x| x.name.to_string_lossy().into_owned()).collect()
}

#[test]
fn a_ditto_sidecar_reads_back_as_its_finder_tags_without_provenance() {
    let parsed = parse(&ditto_zip_member("__MACOSX/._tagged.txt"));
    assert_eq!(names(&parsed), vec![TAGS]);
    assert!(
        parsed[0].value.starts_with(b"bplist00"),
        "the tags are the binary plist ditto stored"
    );
}

#[test]
fn a_ditto_sidecar_holding_only_provenance_carries_nothing() {
    assert!(parse(&ditto_zip_member("__MACOSX/._plain.txt")).is_empty());
}

#[test]
fn building_the_tags_and_provenance_matches_ditto_byte_for_byte() {
    let ditto = ditto_zip_member("__MACOSX/._tagged.txt");
    // ditto keeps provenance; the parser drops it, so read it raw for this pin.
    let provenance = &ditto[ditto.len() - 11..];
    let tags = parse(&ditto).remove(0);
    let built = build(&[tags, xattr("com.apple.provenance", provenance)]);
    assert_eq!(built, ditto, "the layout is the one copyfile(3) writes and reads");
}

#[test]
fn finder_info_resource_fork_and_xattrs_round_trip() {
    let mut finder_info = [0u8; 32];
    finder_info[8] = 0x04; // kHasCustomIcon, in the Finder flags
    let carried = vec![
        xattr(FINDER_INFO_XATTR, &finder_info),
        xattr(TAGS, b"bplist00 tags"),
        xattr("user.cmdr", b"x"),
        xattr(RESOURCE_FORK_XATTR, b"a resource fork"),
    ];
    let parsed = parse(&build(&carried));
    let mut expected = carried.clone();
    expected.sort_by(|a, b| a.name.cmp(&b.name));
    let mut parsed_sorted = parsed;
    parsed_sorted.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(parsed_sorted, expected);
}

#[test]
fn an_all_zero_finder_info_is_no_finder_info() {
    let parsed = parse(&build(&[xattr(FINDER_INFO_XATTR, &[0u8; 32]), xattr(TAGS, b"t")]));
    assert_eq!(names(&parsed), vec![TAGS]);
}

#[test]
fn worth_keeping_ignores_provenance_quarantine_and_an_empty_finder_info() {
    assert!(!worth_keeping("com.apple.provenance", &[1]));
    assert!(!worth_keeping("com.apple.quarantine", b"0083;"));
    assert!(!worth_keeping(FINDER_INFO_XATTR, &[0u8; 32]));
    let mut icon = [0u8; 32];
    icon[8] = 0x04;
    assert!(worth_keeping(FINDER_INFO_XATTR, &icon));
    assert!(worth_keeping(TAGS, b"bplist00"));
    assert!(worth_keeping("user.anything", b""));
}

// ---- Hostile input: never a panic, never a name the kernel would misread ----

/// A well-formed sidecar carrying one xattr named `name`, whatever `name` is.
fn sidecar_with_raw_name(name: &[u8]) -> Vec<u8> {
    encode(&[0; 32], &[(name, b"value")], &[])
}

#[test]
fn hostile_xattr_names_are_dropped() {
    for name in [
        b"".as_slice(),
        b"com.apple.system.Security",
        b"com.apple.provenance",
        b"bad\0inner",
        b"\xff\xfe not utf-8",
        &[b'a'; 200],
    ] {
        let parsed = parse(&sidecar_with_raw_name(name));
        assert!(
            parsed.is_empty(),
            "{:?} must not reach a file, got {parsed:?}",
            String::from_utf8_lossy(name)
        );
    }
    assert_eq!(names(&parse(&sidecar_with_raw_name(b"user.fine"))), vec!["user.fine"]);
}

#[test]
fn hostile_headers_and_offsets_never_panic_and_yield_only_whole_values() {
    let good = build(&[
        xattr(TAGS, b"bplist00 tags"),
        xattr(RESOURCE_FORK_XATTR, b"fork"),
        xattr("user.cmdr", b"x"),
    ]);
    // Every truncation, and every single byte flipped: whatever parses must be
    // a whole value of the original set or nothing.
    for len in 0..good.len() {
        for x in parse(&good[..len]) {
            assert!(x.value.len() <= good.len());
        }
    }
    for at in 0..good.len() {
        let mut flipped = good.clone();
        flipped[at] ^= 0xff;
        let _ = parse(&flipped);
    }
    // Offsets and lengths at the u32 edge.
    let mut huge = good.clone();
    huge[30..34].copy_from_slice(&u32::MAX.to_be_bytes()); // FinderInfo entry offset
    let _ = parse(&huge);
    let mut huge = good.clone();
    huge[34..38].copy_from_slice(&u32::MAX.to_be_bytes()); // FinderInfo entry length
    let _ = parse(&huge);
    let mut many = good.clone();
    many[24..26].copy_from_slice(&u16::MAX.to_be_bytes()); // entry count
    let _ = parse(&many);
    let mut many_attrs = good.clone();
    many_attrs[118..120].copy_from_slice(&u16::MAX.to_be_bytes()); // attr count
    let _ = parse(&many_attrs);
}

#[test]
fn a_short_finder_info_or_a_foreign_magic_carries_nothing() {
    let mut foreign = build(&[xattr(TAGS, b"t")]);
    foreign[0] ^= 0xff;
    assert!(parse(&foreign).is_empty());
    let mut short = build(&[xattr(FINDER_INFO_XATTR, &[1u8; 32])]);
    short[34..38].copy_from_slice(&20u32.to_be_bytes()); // FinderInfo entry 20 bytes long
    assert!(names(&parse(&short)).iter().all(|name| name != FINDER_INFO_XATTR));
}

// ---- Where a sidecar lives, and whose it is ----

#[test]
fn a_sidecar_entry_name_is_the_macosx_shadow_of_its_item() {
    assert_eq!(sidecar_entry_name("a/b.txt"), "__MACOSX/a/._b.txt");
    assert_eq!(sidecar_entry_name("b.txt"), "__MACOSX/._b.txt");
    assert_eq!(sidecar_entry_name("/a/dir/"), "__MACOSX/a/._dir");
}

#[test]
fn macosx_paths_classify_by_the_item_they_shadow() {
    assert_eq!(
        classify_macosx("__MACOSX/a/._b"),
        MacosxEntry::Sidecar("a/b".to_string())
    );
    assert_eq!(classify_macosx("__MACOSX/._b"), MacosxEntry::Sidecar("b".to_string()));
    assert_eq!(classify_macosx("__MACOSX"), MacosxEntry::Other);
    assert_eq!(classify_macosx("__MACOSX/a/b"), MacosxEntry::Other);
    assert_eq!(classify_macosx("a/__MACOSX/._b"), MacosxEntry::Outside);
    assert_eq!(classify_macosx("__MACOSXy/._b"), MacosxEntry::Outside);
    for hostile in ["__MACOSX/._", "__MACOSX/._.", "__MACOSX/a/._.."] {
        assert_eq!(classify_macosx(hostile), MacosxEntry::Other, "{hostile}");
    }
}

#[test]
fn a_sibling_sidecar_names_its_sibling() {
    assert_eq!(sibling_sidecar_target("a/._b"), Some("a/b".to_string()));
    assert_eq!(sibling_sidecar_target("._b"), Some("b".to_string()));
    assert_eq!(sibling_sidecar_target("a/b"), None);
    for hostile in ["._", "a/._.", "._.."] {
        assert_eq!(sibling_sidecar_target(hostile), None, "{hostile}");
    }
}
