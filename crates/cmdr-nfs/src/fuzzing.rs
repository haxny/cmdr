//! Entry points for the fuzz targets in `fuzz/` (repo root).
//!
//! Whatever answers on the server's port writes every byte of a reply record,
//! and the transport hands each record to [`decode_reply`] and then to the
//! called procedure's decoder. The fuzzer is looking for panics, allocations
//! sized by a wire length nothing checked, and hangs; every refusal is already
//! an [`XdrError`](crate::xdr::XdrError) by type.

use bytes::Bytes;

use crate::proto::mount::{decode_exports, decode_mnt};
use crate::proto::nfs3::{
    decode_commit, decode_created, decode_fsinfo, decode_fsstat, decode_getattr, decode_lookup, decode_pathconf,
    decode_read, decode_readdir, decode_readdirplus, decode_write, status_of,
};
use crate::proto::portmap::decode_getport;
use crate::proto::rpc::{ReplyBody, decode_reply};
use crate::transport::MAX_PAYLOAD;
use crate::xdr::Decoder;

/// Decodes `record` as one reply record (no record mark), then its results as
/// every procedure's answer in turn: the record doesn't say which call it
/// answers, so a hostile server gets to pick, and so does the fuzzer.
pub fn reply(record: &[u8]) {
    let Ok((_xid, ReplyBody::Success(mut results))) = decode_reply(Bytes::copy_from_slice(record)) else {
        return;
    };
    let results = results.rest();
    let fresh = || Decoder::new(results.clone());

    let _ = decode_getport(&mut fresh());
    let _ = decode_exports(&mut fresh());
    let _ = decode_mnt(&mut fresh());

    let Ok(Ok(mut ok)) = status_of(fresh()) else {
        return;
    };
    let ok = ok.rest();
    let fresh = || Decoder::new(ok.clone());
    let _ = decode_getattr(&mut fresh());
    let _ = decode_lookup(&mut fresh());
    // The client never asks for more than `MAX_PAYLOAD`, so a longer answer
    // must be refused, ❌ never sliced.
    if let Ok(read) = decode_read(&mut fresh(), MAX_PAYLOAD) {
        assert!(
            read.data.len() <= MAX_PAYLOAD as usize,
            "READ answered past what was asked"
        );
    }
    let _ = decode_write(&mut fresh());
    let _ = decode_created(&mut fresh());
    let _ = decode_readdir(&mut fresh());
    let _ = decode_readdirplus(&mut fresh());
    let _ = decode_fsstat(&mut fresh());
    let _ = decode_fsinfo(&mut fresh());
    let _ = decode_pathconf(&mut fresh());
    let _ = decode_commit(&mut fresh());
}
