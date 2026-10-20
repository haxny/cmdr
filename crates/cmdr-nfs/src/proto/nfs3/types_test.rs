use bytes::Bytes;

use super::*;
use crate::xdr::Encoder;

fn encode_fattr(enc: &mut Encoder, file_type: u32, uid: u32, size: u64) {
    enc.u32(file_type);
    enc.u32(0o644);
    enc.u32(1);
    enc.u32(uid);
    enc.u32(100);
    enc.u64(size);
    enc.u64(size);
    enc.u32(0);
    enc.u32(0);
    enc.u64(7);
    enc.u64(42);
    for seconds in [1, 2, 3] {
        enc.u32(seconds);
        enc.u32(500);
    }
}

fn decoder(write: impl FnOnce(&mut Encoder)) -> Decoder {
    let mut enc = Encoder::new();
    write(&mut enc);
    Decoder::new(enc.finish().freeze())
}

#[test]
fn fattr3_decodes_every_field_in_order() {
    let mut dec = decoder(|enc| encode_fattr(enc, 1, 1000, 12));
    let attrs = decode_fattr(&mut dec).unwrap();
    assert_eq!(attrs.file_type, FileType::Regular);
    assert_eq!((attrs.mode, attrs.uid, attrs.gid, attrs.size), (0o644, 1000, 100, 12));
    assert_eq!((attrs.fsid, attrs.fileid), (7, 42));
    assert_eq!(
        attrs.mtime,
        NfsTime {
            seconds: 2,
            nseconds: 500
        }
    );
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn an_unknown_file_type_is_a_decode_error() {
    let mut dec = decoder(|enc| encode_fattr(enc, 9, 0, 0));
    assert_eq!(
        decode_fattr(&mut dec).unwrap_err(),
        XdrError::BadDiscriminant {
            what: "ftype3",
            value: 9
        }
    );
}

#[test]
fn a_failure_status_stops_before_the_failure_arm() {
    // NFS3ERR_NOENT with a post_op_attr after it that nobody reads.
    let dec = decoder(|enc| {
        enc.u32(2);
        enc.bool(false);
    });
    assert_eq!(status_of(dec).unwrap().unwrap_err(), NfsStat3::NoEnt);
}

#[test]
fn a_readdirplus_page_drops_dot_entries_but_keeps_their_cookie() {
    let mut dec = decoder(|enc| {
        enc.bool(false); // no dir attributes
        enc.fixed_opaque(&[9; 8]);
        // "kept", with attributes and a handle.
        enc.bool(true);
        enc.u64(42);
        enc.opaque(b"kept");
        enc.u64(1);
        enc.bool(true);
        encode_fattr(enc, 1, 1000, 3);
        enc.bool(true);
        enc.opaque(&[0xAB; 8]);
        // "bare", with neither.
        enc.bool(true);
        enc.u64(43);
        enc.opaque(b"bare");
        enc.u64(2);
        enc.bool(false);
        enc.bool(false);
        // "..", last on the page.
        enc.bool(true);
        enc.u64(2);
        enc.opaque(b"..");
        enc.u64(3);
        enc.bool(false);
        enc.bool(false);
        enc.bool(false); // no more entries
        enc.bool(false); // not eof
    });
    let page = decode_readdirplus(&mut dec).unwrap();
    assert_eq!(page.cookieverf, [9; 8]);
    assert_eq!(page.entries.len(), 2);
    assert_eq!(&page.entries[0].name[..], b"kept");
    assert!(page.entries[0].attributes.is_some());
    assert_eq!(page.entries[0].handle, Some(FileHandle(Bytes::from_static(&[0xAB; 8]))));
    assert_eq!(&page.entries[1].name[..], b"bare");
    assert_eq!((page.entries[1].attributes, &page.entries[1].handle), (None, &None));
    // The `..` entry's cookie is where the next page starts.
    assert_eq!(page.next_cookie, Some(3));
    assert!(!page.eof);
}

#[test]
fn a_file_handle_past_64_bytes_is_refused() {
    let mut dec = decoder(|enc| enc.opaque(&[0; 65]));
    assert_eq!(
        decode_handle(&mut dec).unwrap_err(),
        XdrError::TooLong { len: 65, limit: 64 }
    );
}

#[test]
fn a_read_answering_more_than_was_asked_is_refused() {
    let mut dec = decoder(|enc| {
        enc.bool(false);
        enc.u32(8);
        enc.bool(false);
        enc.opaque(&[1; 8]);
    });
    assert_eq!(
        decode_read(&mut dec, 4).unwrap_err(),
        XdrError::TooLong { len: 8, limit: 4 }
    );
}
