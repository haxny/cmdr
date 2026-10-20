//! Real replies from Linux knfsd (the fixture: Alpine 3.24 `nfs-utils` 2.6.4
//! on OrbStack's kernel `7.2.9-orbstack`, captured 2026-10-20 through
//! `dial_fixture_test::capture_knfsd_replies_as_decoding_fixtures`), decoded
//! whole: the reply header, the status, and the success arm, down to the last
//! byte. The `/insecure` export, as uid 1000.

use bytes::Bytes;

use super::mount::{MountStat, decode_exports, decode_mnt};
use super::nfs3::*;
use super::portmap::decode_getport;
use super::rpc::{ReplyBody, decode_reply};
use crate::xdr::Decoder;

fn record(hex: &str) -> Bytes {
    let hex = hex.trim();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    Bytes::from(bytes)
}

/// The results of a captured reply: past the RPC header, at the status.
fn results(hex: &str) -> Decoder {
    match decode_reply(record(hex)).unwrap().1 {
        ReplyBody::Success(results) => results,
        other => panic!("expected an accepted reply, got {other:?}"),
    }
}

/// The success arm of a captured NFS reply.
fn ok(hex: &str) -> Decoder {
    status_of(results(hex)).unwrap().unwrap()
}

#[test]
fn portmap_names_the_published_mount_and_nfs_ports() {
    let mut mount = results(include_str!("testdata/knfsd/portmap-getport-mount.hex"));
    assert_eq!(decode_getport(&mut mount).unwrap(), Some(15482));
    let mut nfs = results(include_str!("testdata/knfsd/portmap-getport-nfs.hex"));
    assert_eq!(decode_getport(&mut nfs).unwrap(), Some(15481));
}

#[test]
fn export_lists_both_exports() {
    let mut dec = results(include_str!("testdata/knfsd/mount-export.hex"));
    let exports = decode_exports(&mut dec).unwrap();
    let mut names: Vec<&[u8]> = exports.iter().map(|e| &e[..]).collect();
    names.sort();
    assert_eq!(names, vec![&b"/insecure"[..], &b"/secure"[..]]);
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn mnt_answers_a_root_handle_and_auth_sys() {
    let mut dec = results(include_str!("testdata/knfsd/mount-mnt-insecure.hex"));
    let mounted = decode_mnt(&mut dec).unwrap().unwrap();
    assert!(!mounted.root.0.is_empty() && mounted.root.0.len() <= MAX_HANDLE);
    assert_eq!(mounted.auth_flavors, vec![1]);
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn mnt_of_secure_through_a_rewritten_port_is_access() {
    let mut dec = results(include_str!("testdata/knfsd/mount-mnt-secure-refused.hex"));
    assert_eq!(decode_mnt(&mut dec).unwrap().unwrap_err(), MountStat::Access);
}

#[test]
fn fsinfo_carries_the_servers_transfer_sizes() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-fsinfo.hex"));
    let info = decode_fsinfo(&mut dec).unwrap();
    assert!(info.rtmax >= info.rtpref && info.rtpref >= 4096, "{info:?}");
    assert!(info.wtmax >= info.wtpref && info.wtpref >= 4096, "{info:?}");
    assert!(info.maxfilesize > 1 << 32);
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn getattr_of_the_root_is_a_world_writable_folder_owned_by_root() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-getattr-root.hex"));
    let root = decode_getattr(&mut dec).unwrap();
    assert_eq!(root.file_type, FileType::Directory);
    assert_eq!(root.mode & 0o7777, 0o777);
    assert_eq!((root.uid, root.gid), (0, 0));
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn readdirplus_of_the_root_carries_every_seeded_name_with_handles_and_attributes() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-readdirplus-root.hex"));
    let page = decode_readdirplus(&mut dec).unwrap();
    assert!(page.eof);
    assert!(page.next_cookie.is_some());
    let names: Vec<&[u8]> = page.entries.iter().map(|e| &e.name[..]).collect();
    for expected in ["hello.txt", "large.bin", "café.txt", "cafe\u{301}.txt", "link-inside"] {
        assert!(names.contains(&expected.as_bytes()), "missing {expected}: {names:?}");
    }
    assert!(!names.contains(&&b"."[..]) && !names.contains(&&b".."[..]));
    for entry in &page.entries {
        assert!(entry.handle.is_some(), "{entry:?}");
        assert!(entry.attributes.is_some(), "{entry:?}");
    }
    let hello = page.entries.iter().find(|e| &e.name[..] == b"hello.txt").unwrap();
    let attrs = hello.attributes.unwrap();
    assert_eq!((attrs.uid, attrs.size, attrs.mode & 0o777), (0, 27, 0o644));
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn readdir_of_the_root_lists_the_same_names_without_attributes() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-readdir-root.hex"));
    let page = decode_readdir(&mut dec).unwrap();
    assert!(page.eof);
    assert!(page.entries.iter().any(|e| &e.name[..] == b"a file with spaces.txt"));
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn lookup_answers_the_handle_and_both_attribute_sets() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-lookup-hello.hex"));
    let found = decode_lookup(&mut dec).unwrap();
    assert_eq!(found.attributes.unwrap().size, 27);
    assert_eq!(found.dir_attributes.unwrap().file_type, FileType::Directory);
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn lookup_of_a_missing_name_is_noent() {
    let status = status_of(results(include_str!("testdata/knfsd/nfs-lookup-missing.hex"))).unwrap();
    assert_eq!(status.unwrap_err(), NfsStat3::NoEnt);
}

#[test]
fn read_slices_the_payload_out_of_the_record() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-read-hello.hex"));
    let read = decode_read(&mut dec, 4096).unwrap();
    assert_eq!(&read.data[..], b"hello from the nfs fixture\n");
    assert!(read.eof);
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn access_grants_read_on_a_world_readable_file() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-access-hello.hex"));
    let _attributes = dec.optional(decode_fattr).unwrap();
    assert_eq!(dec.u32().unwrap() & access::READ, access::READ);
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn readlink_answers_the_relative_target() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-readlink-inside.hex"));
    let _attributes = dec.optional(decode_fattr).unwrap();
    assert_eq!(&dec.opaque(4096).unwrap()[..], b"photos/2024 summer/beach.txt");
}

#[test]
fn fsstat_and_pathconf_decode_to_the_last_byte() {
    let mut dec = ok(include_str!("testdata/knfsd/nfs-fsstat.hex"));
    let stat = decode_fsstat(&mut dec).unwrap();
    assert!(stat.total_bytes >= stat.free_bytes && stat.free_bytes >= stat.available_bytes);
    assert_eq!(dec.remaining(), 0);
    let mut dec = ok(include_str!("testdata/knfsd/nfs-pathconf.hex"));
    let conf = decode_pathconf(&mut dec).unwrap();
    assert_eq!(conf.name_max, 255);
    assert!(!conf.case_insensitive && conf.case_preserving);
    assert_eq!(dec.remaining(), 0);
}
