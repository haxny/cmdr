//! `Nfs3Client`'s own logic against a scripted in-process server: the listing
//! loop (cookies, the verifier echo, `BAD_COOKIE`, the READDIR fallback, the
//! filled-in entries) and the `JUKEBOX` retry.

use std::sync::{Arc, Mutex};

use bytes::Bytes;

use super::*;
use crate::proto::rpc::Credential;
use crate::proto::server::nfs3_wire::{fattr, handle as wire_handle, post_op_attr, post_op_fh, simple_attrs};
use crate::proto::server::{CallFrame, ServerConn, success_reply};
use crate::transport::{Probe, ReservedPort, TransportOptions};

const DIR: &[u8] = b"dir-handle";

fn fh(bytes: &'static [u8]) -> FileHandle {
    FileHandle(Bytes::from_static(bytes))
}

/// Serves calls with `answer` until the client goes away. `answer` gets each
/// call and writes the results (status included), or `None` to stay silent.
fn serve(
    mut answer: impl FnMut(&mut CallFrame) -> Option<Box<dyn FnOnce(&mut Encoder) + Send>> + Send + 'static,
) -> Nfs3Client {
    let (client, server) = tokio::io::duplex(1 << 20);
    let probe: Probe = Arc::new(|| Box::pin(async { true }));
    let conn = RpcConnection::over_stream(
        client,
        Credential::None,
        probe,
        ReservedPort::NotAsked,
        TransportOptions::PRODUCTION,
    );
    tokio::spawn(async move {
        let mut server = ServerConn::new(server);
        while let Some(mut call) = server.next_call().await {
            if let Some(results) = answer(&mut call) {
                let reply = success_reply(call.xid, results);
                server.send(&reply).await;
            }
        }
    });
    Nfs3Client::new(conn, Some(Duration::from_secs(60)))
}

/// What a READDIR/READDIRPLUS call asked for: its cookie and verifier.
fn page_args(call: &mut CallFrame) -> (u64, [u8; 8]) {
    let _dir = call.args.opaque(64).unwrap();
    (call.args.u64().unwrap(), call.args.fixed_array().unwrap())
}

fn status(enc: &mut Encoder, number: u32) {
    enc.u32(number);
}

fn collect(events: Vec<ListingEvent>) -> Vec<String> {
    let mut names = Vec::new();
    for event in events {
        match event {
            ListingEvent::Entries(batch) => {
                names.extend(batch.into_iter().map(|e| String::from_utf8(e.name.to_vec()).unwrap()));
            }
            ListingEvent::Restarted => names.push("<restart>".to_string()),
        }
    }
    names
}

#[tokio::test]
async fn without_readdirplus_the_listing_falls_back_to_readdir_and_pipelined_lookups() {
    let seen_pages = Arc::new(Mutex::new(Vec::new()));
    let pages = Arc::clone(&seen_pages);
    let nfs = serve(move |call| match call.procedure {
        17 => Some(Box::new(|enc| {
            status(enc, 10004); // NOTSUPP
            enc.bool(false);
        })),
        16 => {
            let (cookie, verf) = page_args(call);
            pages.lock().unwrap().push((cookie, verf));
            Some(Box::new(move |enc| {
                status(enc, 0);
                enc.bool(false);
                enc.fixed_opaque(&[7; 8]);
                let names: &[(&[u8], u64)] = if cookie == 0 {
                    &[(b".", 1), (b"a", 2), (b"gone", 3), (b"..", 4)]
                } else {
                    &[(b"bare", 5)]
                };
                for (name, next) in names {
                    enc.bool(true);
                    enc.u64(*next);
                    enc.opaque(name);
                    enc.u64(*next);
                }
                enc.bool(false);
                enc.bool(cookie != 0);
            }))
        }
        3 => {
            let _dir = call.args.opaque(64).unwrap();
            let name = call.args.opaque(255).unwrap();
            Some(Box::new(move |enc| match &name[..] {
                b"gone" => {
                    status(enc, 2); // NOENT: deleted between the page and the lookup
                    post_op_attr(enc, None);
                }
                b"bare" => {
                    status(enc, 0);
                    wire_handle(enc, &fh(b"bare"));
                    post_op_attr(enc, None); // no attributes: a GETATTR follows
                    post_op_attr(enc, None);
                }
                _ => {
                    status(enc, 0);
                    wire_handle(enc, &FileHandle(name.clone()));
                    post_op_attr(enc, Some(&simple_attrs(FileType::Regular, 2, 10)));
                    post_op_attr(enc, None);
                }
            }))
        }
        1 => Some(Box::new(|enc| {
            status(enc, 0);
            fattr(enc, &simple_attrs(FileType::Directory, 5, 0));
        })),
        other => panic!("unexpected procedure {other}"),
    });

    let mut events = Vec::new();
    nfs.list_dir(&fh(DIR), 8192, |event| events.push(event)).await.unwrap();
    let names = collect(events);
    assert_eq!(names, vec!["a", "bare"]);
    // The second page starts at the `..` entry's cookie, echoing the verifier.
    assert_eq!(*seen_pages.lock().unwrap(), vec![(0, [0; 8]), (4, [7; 8])]);
}

#[tokio::test]
async fn an_entry_listed_without_a_handle_is_looked_up_and_bad_cookie_restarts_the_listing() {
    let calls = Arc::new(Mutex::new(0u32));
    let count = Arc::clone(&calls);
    let nfs = serve(move |call| match call.procedure {
        17 => {
            let (cookie, _) = page_args(call);
            let mut n = count.lock().unwrap();
            *n += 1;
            let attempt = *n;
            Some(Box::new(move |enc| {
                if cookie != 0 && attempt == 2 {
                    status(enc, 10003); // BAD_COOKIE on the first second page
                    enc.bool(false);
                    return;
                }
                status(enc, 0);
                enc.bool(false);
                enc.fixed_opaque(&[1; 8]);
                let entries: &[(&[u8], bool)] = if cookie == 0 {
                    &[(b"full", true), (b"nohandle", false)]
                } else {
                    &[(b"last", true)]
                };
                for (i, (name, complete)) in entries.iter().enumerate() {
                    enc.bool(true);
                    enc.u64(10 + i as u64);
                    enc.opaque(name);
                    enc.u64(10 + i as u64 + cookie);
                    post_op_attr(enc, Some(&simple_attrs(FileType::Regular, 1, 1)));
                    post_op_fh(enc, complete.then(|| fh(b"h")).as_ref());
                }
                enc.bool(false);
                enc.bool(cookie != 0);
            }))
        }
        3 => Some(Box::new(|enc| {
            status(enc, 0);
            wire_handle(enc, &fh(b"looked-up"));
            post_op_attr(enc, Some(&simple_attrs(FileType::Regular, 2, 2)));
            post_op_attr(enc, None);
        })),
        other => panic!("unexpected procedure {other}"),
    });

    let mut events = Vec::new();
    nfs.list_dir(&fh(DIR), 8192, |event| events.push(event)).await.unwrap();
    assert_eq!(
        collect(events),
        vec!["full", "nohandle", "<restart>", "full", "nohandle", "last"]
    );
}

/// A poller compares a folder's stamp against what it was when the pane
/// listed it, so the listing answers the folder's attributes from its FIRST
/// page: a change during a later page then moves the stamp past it, ❌ never
/// hides behind it.
#[tokio::test]
async fn a_listing_answers_the_folder_attributes_its_first_page_carried() {
    let nfs = serve(move |call| match call.procedure {
        17 => {
            let (cookie, _) = page_args(call);
            Some(Box::new(move |enc| {
                status(enc, 0);
                // The folder as the first page saw it (size 1), then as the
                // second did (size 2), after a name inside it changed.
                let size = if cookie == 0 { 1 } else { 2 };
                post_op_attr(enc, Some(&simple_attrs(FileType::Directory, 5, size)));
                enc.fixed_opaque(&[1; 8]);
                enc.bool(true);
                enc.u64(20);
                enc.opaque(if cookie == 0 { b"one" } else { b"two" });
                enc.u64(cookie + 1);
                post_op_attr(enc, Some(&simple_attrs(FileType::Regular, 1, 1)));
                post_op_fh(enc, Some(&fh(b"h")));
                enc.bool(false);
                enc.bool(cookie != 0);
            }))
        }
        other => panic!("unexpected procedure {other}"),
    });

    let folder = nfs.list_dir(&fh(DIR), 8192, |_| {}).await.unwrap();

    assert_eq!(
        folder.map(|attrs| attrs.size),
        Some(1),
        "the first page's, ❌ not the last's"
    );
}

#[tokio::test]
async fn a_page_with_no_entries_and_no_end_is_refused_rather_than_asked_forever() {
    let nfs = serve(|_| {
        Some(Box::new(|enc| {
            status(enc, 0);
            enc.bool(false);
            enc.fixed_opaque(&[0; 8]);
            enc.bool(false);
            enc.bool(false);
        }))
    });
    let err = nfs.list_dir(&fh(DIR), 8192, |_| {}).await.unwrap_err();
    assert_eq!(err, NfsError::NoProgress);
}

#[tokio::test(start_paused = true)]
async fn jukebox_is_retried_after_a_second_up_to_three_times() {
    let asked = Arc::new(Mutex::new(0u32));
    let count = Arc::clone(&asked);
    let nfs = serve(move |_| {
        let mut n = count.lock().unwrap();
        *n += 1;
        let attempt = *n;
        Some(Box::new(move |enc| {
            if attempt <= 3 {
                status(enc, 10008);
                enc.bool(false);
            } else {
                status(enc, 0);
                fattr(enc, &simple_attrs(FileType::Regular, 9, 3));
            }
        }))
    });
    let started = tokio::time::Instant::now();
    let attrs = nfs.getattr(&fh(b"f")).await.unwrap();
    assert_eq!(attrs.fileid, 9);
    assert_eq!(*asked.lock().unwrap(), 4);
    assert!(started.elapsed() >= Duration::from_secs(3));
}

#[tokio::test(start_paused = true)]
async fn jukebox_past_the_retries_reaches_the_caller() {
    let nfs = serve(|_| {
        Some(Box::new(|enc| {
            status(enc, 10008);
            enc.bool(false);
        }))
    });
    assert_eq!(
        nfs.getattr(&fh(b"f")).await.unwrap_err(),
        NfsError::Status(NfsStat3::Jukebox)
    );
}

#[tokio::test]
async fn a_write_carries_its_offset_stability_and_bytes() {
    let seen = Arc::new(Mutex::new(None));
    let record = Arc::clone(&seen);
    let nfs = serve(move |call| {
        let _fh = call.args.opaque(64).unwrap();
        let offset = call.args.u64().unwrap();
        let count = call.args.u32().unwrap();
        let stable = call.args.u32().unwrap();
        let data = call.args.opaque(1 << 20).unwrap();
        *record.lock().unwrap() = Some((offset, count, stable, data));
        Some(Box::new(|enc| {
            status(enc, 0);
            enc.bool(false);
            enc.bool(false);
            enc.u32(3);
            enc.u32(0);
            enc.fixed_opaque(&[5; 8]);
        }))
    });
    let written = nfs.write(&fh(b"f"), 4096, Stable::Unstable, b"abcde").await.unwrap();
    // A short count comes back as it stands: resending the rest is the caller's.
    assert_eq!(
        (written.count, written.committed, written.verifier),
        (3, Stable::Unstable, [5; 8])
    );
    let (offset, count, stable, data) = seen.lock().unwrap().take().unwrap();
    assert_eq!((offset, count, stable, &data[..]), (4096, 5, 0, &b"abcde"[..]));
}
