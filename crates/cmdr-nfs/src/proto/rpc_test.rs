use bytes::Bytes;

use super::*;
use crate::xdr::Encoder;

const NFS: Program = Program {
    number: 100_003,
    version: 3,
};

fn getattr() -> Procedure {
    Procedure {
        program: NFS,
        number: 1,
        repeat: Repeat::Safe,
    }
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|w| u32::from_be_bytes([w[0], w[1], w[2], w[3]]))
        .collect()
}

#[test]
fn a_call_header_with_no_credential_is_ten_words() {
    let mut enc = Encoder::new();
    encode_call_header(&mut enc, 0x1234, getattr(), &Credential::None);
    let bytes = enc.finish();
    // xid, CALL, rpcvers 2, prog, vers, proc, cred flavor + empty body,
    // verifier flavor + empty body.
    assert_eq!(words(&bytes), vec![0x1234, 0, 2, 100_003, 3, 1, 0, 0, 0, 0]);
}

#[test]
fn an_auth_sys_credential_carries_the_identity_and_an_auth_none_verifier() {
    let cred = Credential::sys(&AuthSys {
        stamp: 7,
        machine_name: "mac".to_string(),
        uid: 1000,
        gid: 100,
        gids: vec![100],
    });
    let mut enc = Encoder::new();
    encode_call_header(&mut enc, 1, getattr(), &cred);
    let w = words(&enc.finish());
    assert_eq!(w[6], AUTH_SYS);
    // Body: stamp, name length 3, "mac\0", uid, gid, one gid.
    assert_eq!(w[7], 4 * 7);
    assert_eq!(&w[8..10], &[7, 3]);
    assert_eq!(w[10].to_be_bytes(), *b"mac\0");
    assert_eq!(&w[11..14], &[1000, 100, 1]);
    assert_eq!(w[14], 100);
    // The verifier is AUTH_NONE.
    assert_eq!(&w[15..], &[0, 0]);
}

#[test]
fn an_overlong_machine_name_and_group_list_are_cut_to_the_rfc_caps() {
    let Credential::Sys(body) = Credential::sys(&AuthSys {
        stamp: 0,
        // 300 bytes of a two-byte character, so the cut has to find a boundary.
        machine_name: "é".repeat(150),
        uid: 1,
        gid: 1,
        gids: (0..40).collect(),
    }) else {
        panic!("expected AUTH_SYS");
    };
    let mut dec = Decoder::new(body.clone());
    dec.u32().unwrap();
    let name = dec.opaque(255).unwrap();
    assert_eq!(name.len(), 254);
    assert!(std::str::from_utf8(&name).is_ok());
    dec.u32().unwrap();
    dec.u32().unwrap();
    assert_eq!(dec.array_len(16, 4).unwrap(), 16);
    assert!(body.len() <= 400);
}

fn reply(words: &[u32]) -> Bytes {
    let mut enc = Encoder::new();
    for w in words {
        enc.u32(*w);
    }
    enc.finish().freeze()
}

#[test]
fn a_successful_reply_hands_back_the_results_after_the_verifier() {
    let (xid, body) = decode_reply(reply(&[9, REPLY, 0, 0, 0, 0, 42])).unwrap();
    assert_eq!(xid, 9);
    let ReplyBody::Success(mut results) = body else {
        panic!("expected success, got {body:?}");
    };
    assert_eq!(results.u32().unwrap(), 42);
}

#[test]
fn every_accept_stat_decodes_by_number() {
    let cases = [
        (vec![1], AcceptError::ProgramUnavailable),
        (vec![2, 2, 4], AcceptError::ProgramMismatch { low: 2, high: 4 }),
        (vec![3], AcceptError::ProcedureUnavailable),
        (vec![4], AcceptError::GarbageArgs),
        (vec![5], AcceptError::SystemError),
        (vec![77], AcceptError::Unknown(77)),
    ];
    for (tail, expected) in cases {
        let mut w = vec![1, REPLY, 0, 0, 0];
        w.extend(tail);
        let (_, body) = decode_reply(reply(&w)).unwrap();
        assert!(matches!(body, ReplyBody::NotAccepted(e) if e == expected), "{body:?}");
    }
}

#[test]
fn a_denial_carries_its_reject_and_auth_stat() {
    let (_, body) = decode_reply(reply(&[1, REPLY, 1, 1, 5])).unwrap();
    assert!(matches!(body, ReplyBody::Denied(Rejection::Auth(AuthStat::TooWeak))));
    let (_, body) = decode_reply(reply(&[1, REPLY, 1, 1, 1])).unwrap();
    assert!(matches!(
        body,
        ReplyBody::Denied(Rejection::Auth(AuthStat::BadCredential))
    ));
    let (_, body) = decode_reply(reply(&[1, REPLY, 1, 0, 2, 2])).unwrap();
    assert!(matches!(
        body,
        ReplyBody::Denied(Rejection::RpcMismatch { low: 2, high: 2 })
    ));
}

#[test]
fn a_call_where_a_reply_belongs_is_a_decode_error() {
    let err = decode_reply(reply(&[1, 0, 0])).unwrap_err();
    assert_eq!(
        err,
        XdrError::BadDiscriminant {
            what: "msg_type",
            value: 0
        }
    );
}

#[test]
fn peek_xid_needs_four_bytes() {
    assert_eq!(peek_xid(&reply(&[0xABCD])), Some(0xABCD));
    assert_eq!(peek_xid(&Bytes::from_static(&[1, 2])), None);
}
