use bytes::Bytes;

use super::*;

fn decoder(bytes: &[u8]) -> Decoder {
    Decoder::new(Bytes::copy_from_slice(bytes))
}

#[test]
fn scalars_round_trip_big_endian() {
    let mut enc = Encoder::new();
    enc.u32(0xDEAD_BEEF);
    enc.i32(-2);
    enc.u64(0x0102_0304_0506_0708);
    enc.bool(true);
    enc.bool(false);
    let bytes = enc.finish().freeze();
    assert_eq!(&bytes[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);

    let mut dec = Decoder::new(bytes);
    assert_eq!(dec.u32().unwrap(), 0xDEAD_BEEF);
    assert_eq!(dec.i32().unwrap(), -2);
    assert_eq!(dec.u64().unwrap(), 0x0102_0304_0506_0708);
    assert!(dec.bool().unwrap());
    assert!(!dec.bool().unwrap());
    assert_eq!(dec.remaining(), 0);
}

#[test]
fn opaque_pads_to_four_bytes_both_ways() {
    let mut enc = Encoder::new();
    enc.opaque(b"abcde");
    enc.u32(7);
    let bytes = enc.finish().freeze();
    // Length, five bytes, three of padding, then the next value.
    assert_eq!(bytes.len(), 4 + 8 + 4);
    assert_eq!(&bytes[4..12], b"abcde\0\0\0");

    let mut dec = Decoder::new(bytes);
    assert_eq!(&dec.opaque(64).unwrap()[..], b"abcde");
    assert_eq!(dec.u32().unwrap(), 7);
}

#[test]
fn fixed_opaque_has_no_length_word() {
    let mut enc = Encoder::new();
    enc.fixed_opaque(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let bytes = enc.finish().freeze();
    assert_eq!(bytes.len(), 8);
    let mut dec = Decoder::new(bytes);
    assert_eq!(dec.fixed_array::<8>().unwrap(), [1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn an_opaque_slice_shares_the_record_instead_of_copying_it() {
    let mut enc = Encoder::new();
    enc.opaque(&[9u8; 1024]);
    let record = enc.finish().freeze();
    let start = record.as_ptr() as usize;
    let mut dec = Decoder::new(record);
    let payload = dec.opaque(4096).unwrap();
    assert_eq!(payload.as_ptr() as usize, start + 4);
}

#[test]
fn a_four_gib_length_in_a_hundred_byte_record_fails_without_allocating() {
    // The hostile case the bounded decoder exists for: the length word claims
    // nearly 4 GiB, and the record holds 96 bytes after it.
    let mut bytes = vec![0xFF, 0xFF, 0xFF, 0xFC];
    bytes.extend_from_slice(&[0u8; 96]);
    let err = decoder(&bytes).opaque(usize::MAX).unwrap_err();
    assert_eq!(
        err,
        XdrError::Truncated {
            needed: 0xFFFF_FFFC,
            left: 96
        }
    );
}

#[test]
fn a_length_past_the_fields_own_limit_is_refused_before_the_record_is_consulted() {
    let mut enc = Encoder::new();
    enc.opaque(&[0u8; 65]);
    let err = Decoder::new(enc.finish().freeze()).opaque(64).unwrap_err();
    assert_eq!(err, XdrError::TooLong { len: 65, limit: 64 });
}

#[test]
fn a_missing_pad_is_a_truncation() {
    // Length 5, five bytes, and the record ends where the padding should be.
    let err = decoder(&[0, 0, 0, 5, b'a', b'b', b'c', b'd', b'e'])
        .opaque(64)
        .unwrap_err();
    assert_eq!(err, XdrError::Truncated { needed: 8, left: 5 });
}

#[test]
fn a_short_scalar_is_a_truncation() {
    assert_eq!(
        decoder(&[0, 0]).u32().unwrap_err(),
        XdrError::Truncated { needed: 4, left: 2 }
    );
    assert_eq!(
        decoder(&[0, 0, 0, 0, 0]).u64().unwrap_err(),
        XdrError::Truncated { needed: 8, left: 5 }
    );
}

#[test]
fn a_bool_is_zero_or_one_and_nothing_else() {
    assert_eq!(
        decoder(&[0, 0, 0, 2]).bool().unwrap_err(),
        XdrError::BadDiscriminant { what: "bool", value: 2 }
    );
}

#[test]
fn an_array_count_is_checked_against_the_smallest_element_before_any_capacity() {
    // 1,000,000 four-byte elements claimed, eight bytes present.
    let err = decoder(&[0, 0x0F, 0x42, 0x40, 0, 0, 0, 1, 0, 0, 0, 2])
        .array_len(usize::MAX, 4)
        .unwrap_err();
    assert_eq!(
        err,
        XdrError::Truncated {
            needed: 4_000_000,
            left: 8
        }
    );
    assert_eq!(
        decoder(&[0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2]).array_len(16, 4).unwrap(),
        2
    );
    assert_eq!(
        decoder(&[0, 0, 0, 17]).array_len(16, 4).unwrap_err(),
        XdrError::TooLong { len: 17, limit: 16 }
    );
}

#[test]
fn optional_reads_the_presence_word_first() {
    let mut enc = Encoder::new();
    enc.bool(true);
    enc.u32(5);
    enc.bool(false);
    let mut dec = Decoder::new(enc.finish().freeze());
    assert_eq!(dec.optional(Decoder::u32).unwrap(), Some(5));
    assert_eq!(dec.optional(Decoder::u32).unwrap(), None);
}
