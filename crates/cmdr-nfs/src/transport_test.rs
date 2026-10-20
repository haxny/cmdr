use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::net::TcpListener;

use super::*;
use crate::proto::rpc::{Program, Repeat};
use crate::proto::server::{ServerConn, decode_call, success_reply};

const PROGRAM: Program = Program {
    number: 100_003,
    version: 3,
};

fn procedure(number: u32, repeat: Repeat) -> Procedure {
    Procedure {
        program: PROGRAM,
        number,
        repeat,
    }
}

fn answering_probe(answers: bool) -> (Probe, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    let probe: Probe = Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { answers })
    });
    (probe, asked)
}

fn pair_with(probe: Probe, options: TransportOptions) -> (RpcConnection, ServerConn<DuplexStream>) {
    let (client, server) = tokio::io::duplex(1 << 20);
    let conn = RpcConnection::over_stream(client, Credential::None, probe, ReservedPort::NotAsked, options);
    (conn, ServerConn::new(server))
}

fn pair() -> (RpcConnection, ServerConn<DuplexStream>) {
    pair_with(answering_probe(true).0, TransportOptions::PRODUCTION)
}

/// Sends `number` with a one-word argument and answers the word the reply
/// carried.
async fn ask(conn: &RpcConnection, number: u32, repeat: Repeat) -> Result<u32, CallError> {
    let mut results = conn
        .call(procedure(number, repeat), |enc| enc.u32(number), None)
        .await?;
    Ok(results.u32()?)
}

fn spawn_ask(conn: &RpcConnection, number: u32, repeat: Repeat) -> tokio::task::JoinHandle<Result<u32, CallError>> {
    let conn = conn.clone();
    tokio::spawn(async move { ask(&conn, number, repeat).await })
}

#[tokio::test]
async fn pipelined_calls_answered_out_of_order_each_reach_their_caller() {
    let (conn, mut server) = pair();
    let first = spawn_ask(&conn, 1, Repeat::Safe);
    let second = spawn_ask(&conn, 2, Repeat::Safe);
    let third = spawn_ask(&conn, 3, Repeat::Safe);
    let mut calls = Vec::new();
    for _ in 0..3 {
        calls.push(server.next_call().await.unwrap());
    }
    // Answer in reverse, each with ten times its procedure number.
    for call in calls.iter().rev() {
        server.reply(call.xid, |enc| enc.u32(call.procedure * 10)).await;
    }
    assert_eq!(first.await.unwrap().unwrap(), 10);
    assert_eq!(second.await.unwrap().unwrap(), 20);
    assert_eq!(third.await.unwrap().unwrap(), 30);
}

#[tokio::test]
async fn a_cancelled_call_forgets_its_xid_and_leaves_the_connection_usable() {
    let (conn, mut server) = pair();
    let cancelled = spawn_ask(&conn, 1, Repeat::Safe);
    let stale = server.next_call().await.unwrap();
    cancelled.abort();
    let _ = cancelled.await;
    assert_eq!(conn.waiting_calls(), 0);
    // The late reply arrives for nobody and is dropped.
    server.reply(stale.xid, |enc| enc.u32(99)).await;

    let next = spawn_ask(&conn, 2, Repeat::Safe);
    let call = server.next_call().await.unwrap();
    assert_ne!(call.xid, stale.xid);
    server.reply(call.xid, |enc| enc.u32(20)).await;
    assert_eq!(next.await.unwrap().unwrap(), 20);
    assert!(conn.is_usable());
}

#[tokio::test]
async fn a_reply_split_across_fragments_is_reassembled() {
    let (conn, mut server) = pair();
    let pending = spawn_ask(&conn, 4, Repeat::Safe);
    let call = server.next_call().await.unwrap();
    let whole = success_reply(call.xid, |enc| enc.u32(40));
    let body = &whole[4..];
    // Two fragments: the first without the last-fragment bit.
    let (head, tail) = body.split_at(10);
    let mut wire = Vec::new();
    wire.extend_from_slice(&(head.len() as u32).to_be_bytes());
    wire.extend_from_slice(head);
    wire.extend_from_slice(&((tail.len() as u32) | 0x8000_0000).to_be_bytes());
    wire.extend_from_slice(tail);
    server.send(&wire).await;
    assert_eq!(pending.await.unwrap().unwrap(), 40);
}

#[tokio::test]
async fn a_reply_that_wont_decode_fails_only_its_own_call() {
    let (conn, mut server) = pair();
    let broken = spawn_ask(&conn, 1, Repeat::Safe);
    let fine = spawn_ask(&conn, 2, Repeat::Safe);
    let a = server.next_call().await.unwrap();
    let b = server.next_call().await.unwrap();
    let (broken_call, fine_call) = if a.procedure == 1 { (a, b) } else { (b, a) };
    // Well framed, but `reply_stat` 7 is no such thing.
    let garbage = frame_record(|enc| {
        enc.u32(broken_call.xid);
        enc.u32(1);
        enc.u32(7);
    });
    server.send(&garbage).await;
    server.reply(fine_call.xid, |enc| enc.u32(20)).await;
    assert!(matches!(broken.await.unwrap(), Err(CallError::Decode(_))));
    assert_eq!(fine.await.unwrap().unwrap(), 20);
    assert!(conn.is_usable());
}

#[tokio::test]
async fn a_reset_mid_record_answers_every_waiting_call_at_once() {
    let (conn, mut server) = pair();
    let first = spawn_ask(&conn, 1, Repeat::Safe);
    let second = spawn_ask(&conn, 2, Repeat::Never);
    let call = server.next_call().await.unwrap();
    server.next_call().await.unwrap();
    let reply = success_reply(call.xid, |enc| enc.u32(10));
    server.send(&reply[..reply.len() - 3]).await;
    drop(server);
    assert!(matches!(first.await.unwrap(), Err(CallError::Disconnected(_))));
    assert!(matches!(second.await.unwrap(), Err(CallError::Disconnected(_))));
    assert!(conn.lost().is_cancelled());
    assert!(matches!(
        ask(&conn, 3, Repeat::Safe).await,
        Err(CallError::Disconnected(_))
    ));
}

#[tokio::test]
async fn a_fragment_past_the_cap_poisons_the_connection_without_buffering_it() {
    let options = TransportOptions {
        record_cap: 1024,
        ..TransportOptions::PRODUCTION
    };
    let (conn, mut server) = pair_with(answering_probe(true).0, options);
    let pending = spawn_ask(&conn, 1, Repeat::Safe);
    server.next_call().await.unwrap();
    // A last fragment claiming 2 GiB, and nothing after the mark.
    server.send(&0xFFFF_FFFFu32.to_be_bytes()).await;
    assert!(matches!(pending.await.unwrap(), Err(CallError::Disconnected(_))));
    assert!(!conn.is_usable());
}

#[tokio::test]
async fn record_marks_that_sum_past_the_cap_are_refused_too() {
    let mut wire = Vec::new();
    // Three non-last fragments of 400 bytes against a 1,000-byte cap.
    for _ in 0..3 {
        wire.extend_from_slice(&400u32.to_be_bytes());
        wire.extend_from_slice(&[0u8; 400]);
    }
    let err = read_record(&mut wire.as_slice(), 1000, &|| {}).await.unwrap_err();
    assert!(matches!(err, FrameError::TooLarge { len: 1200, cap: 1000 }), "{err:?}");
}

#[tokio::test]
async fn a_truncated_record_is_a_mid_record_close() {
    let mut wire = Vec::new();
    wire.extend_from_slice(&(100u32 | 0x8000_0000).to_be_bytes());
    wire.extend_from_slice(&[0u8; 40]);
    let err = read_record(&mut wire.as_slice(), 1000, &|| {}).await.unwrap_err();
    assert!(matches!(err, FrameError::ClosedMidRecord), "{err:?}");
    let err = read_record(&mut [0u8, 0].as_slice(), 1000, &|| {}).await.unwrap_err();
    assert!(matches!(err, FrameError::ClosedMidRecord), "{err:?}");
    let err = read_record(&mut [0u8; 0].as_slice(), 1000, &|| {}).await.unwrap_err();
    assert!(matches!(err, FrameError::Closed), "{err:?}");
}

#[tokio::test(start_paused = true)]
async fn a_budget_that_runs_out_answers_timed_out() {
    let (conn, mut server) = pair();
    let pending = {
        let conn = conn.clone();
        tokio::spawn(async move {
            conn.call(procedure(1, Repeat::Safe), |_| {}, Some(Duration::from_secs(60)))
                .await
                .map(|_| ())
        })
    };
    server.next_call().await.unwrap();
    assert_eq!(pending.await.unwrap(), Err(CallError::TimedOut));
    assert!(conn.is_usable());
}

#[tokio::test(start_paused = true)]
async fn a_dropped_reply_is_resent_once_with_the_same_xid_once_a_later_call_is_answered() {
    let (conn, mut server) = pair();
    let dropped = spawn_ask(&conn, 1, Repeat::Safe);
    let original = server.next_call().await.unwrap();
    let later = spawn_ask(&conn, 2, Repeat::Safe);
    let later_call = server.next_call().await.unwrap();
    server.reply(later_call.xid, |enc| enc.u32(20)).await;
    assert_eq!(later.await.unwrap().unwrap(), 20);

    // The server never answers the first: 60 s on, it comes again, same XID.
    let resent = server.next_call().await.unwrap();
    assert_eq!(resent.xid, original.xid);
    assert_eq!(resent.procedure, 1);
    server.reply(resent.xid, |enc| enc.u32(10)).await;
    assert_eq!(dropped.await.unwrap().unwrap(), 10);
}

#[tokio::test(start_paused = true)]
async fn a_stuck_call_is_not_resent_while_nothing_after_it_has_been_answered() {
    // The whole server is slow, not this one call: the silence ladder's case,
    // and the probe keeps saying the server is there.
    let (conn, mut server) = pair();
    let slow = spawn_ask(&conn, 1, Repeat::Safe);
    let call = server.next_call().await.unwrap();
    // allowed-test-sleep: a paused clock; five stuck windows passing IS the scenario
    tokio::time::sleep(Duration::from_secs(300)).await;
    server.reply(call.xid, |enc| enc.u32(10)).await;
    assert_eq!(slow.await.unwrap().unwrap(), 10);
    // Nothing else arrived in the meantime: the only record after the call is
    // none at all.
    drop(conn);
    assert!(server.next_call().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn a_stuck_call_that_cant_be_resent_fails_and_retires_the_connection() {
    let (conn, mut server) = pair();
    let create = spawn_ask(&conn, 8, Repeat::Never);
    server.next_call().await.unwrap();
    let later = spawn_ask(&conn, 1, Repeat::Safe);
    let later_call = server.next_call().await.unwrap();
    server.reply(later_call.xid, |enc| enc.u32(10)).await;
    assert_eq!(later.await.unwrap().unwrap(), 10);

    assert_eq!(create.await.unwrap(), Err(CallError::Unanswered));
    assert!(!conn.is_usable());
    assert_eq!(ask(&conn, 1, Repeat::Safe).await, Err(CallError::Retired));
    // Retiring isn't losing: nothing else was cut.
    assert!(!conn.lost().is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_resent_call_that_goes_unanswered_again_poisons_the_connection() {
    let (conn, mut server) = pair();
    let dropped = spawn_ask(&conn, 1, Repeat::Safe);
    server.next_call().await.unwrap();
    let later = spawn_ask(&conn, 2, Repeat::Safe);
    let later_call = server.next_call().await.unwrap();
    server.reply(later_call.xid, |enc| enc.u32(20)).await;
    later.await.unwrap().unwrap();

    let resent = server.next_call().await.unwrap();
    assert_eq!(resent.procedure, 1);
    // A call after the resend is answered; the resent one never is.
    let after = spawn_ask(&conn, 3, Repeat::Safe);
    let after_call = server.next_call().await.unwrap();
    server.reply(after_call.xid, |enc| enc.u32(30)).await;
    after.await.unwrap().unwrap();

    assert!(matches!(dropped.await.unwrap(), Err(CallError::Disconnected(_))));
    assert!(conn.lost().is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_silent_server_is_given_up_on_inside_the_ladder() {
    let (probe, asked) = answering_probe(false);
    let (conn, mut server) = pair_with(probe, TransportOptions::PRODUCTION);
    let started = Instant::now();
    let waiting = spawn_ask(&conn, 1, Repeat::Safe);
    server.next_call().await.unwrap();
    assert!(matches!(waiting.await.unwrap(), Err(CallError::Disconnected(_))));
    // 5 s quiet, then two probes of 5 s each.
    assert!(started.elapsed() <= Duration::from_secs(16), "{:?}", started.elapsed());
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    assert!(conn.lost().is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_busy_server_that_answers_the_probe_keeps_the_call_waiting() {
    let (probe, asked) = answering_probe(true);
    let (conn, mut server) = pair_with(probe, TransportOptions::PRODUCTION);
    let waiting = spawn_ask(&conn, 1, Repeat::Safe);
    let call = server.next_call().await.unwrap();
    // allowed-test-sleep: a paused clock; nine quiet windows passing IS the scenario
    tokio::time::sleep(Duration::from_secs(45)).await;
    assert!(asked.load(Ordering::SeqCst) >= 2);
    server.reply(call.xid, |enc| enc.u32(10)).await;
    assert_eq!(waiting.await.unwrap().unwrap(), 10);
}

#[tokio::test]
async fn close_answers_every_waiting_call() {
    let (conn, mut server) = pair();
    let waiting = spawn_ask(&conn, 1, Repeat::Safe);
    server.next_call().await.unwrap();
    conn.close();
    assert!(matches!(waiting.await.unwrap(), Err(CallError::Disconnected(_))));
}

#[tokio::test]
async fn xids_never_repeat_across_connections() {
    let (first, mut first_server) = pair();
    let (second, mut second_server) = pair();
    let _a = spawn_ask(&first, 1, Repeat::Safe);
    let _b = spawn_ask(&second, 1, Repeat::Safe);
    let a = first_server.next_call().await.unwrap();
    let b = second_server.next_call().await.unwrap();
    assert_ne!(a.xid, b.xid);
}

#[tokio::test]
async fn a_reserved_source_port_is_below_1024_and_is_what_the_server_sees() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let accepting = tokio::spawn(async move { listener.accept().await.unwrap() });
    let conn = RpcConnection::connect(
        target,
        PROGRAM,
        Credential::None,
        SourcePort::Reserved,
        TransportOptions::PRODUCTION,
    )
    .await
    .unwrap();
    let (_stream, peer) = accepting.await.unwrap();
    match conn.reserved_port() {
        ReservedPort::Bound(port) => {
            assert!(port < 1024, "bound {port}");
            assert_eq!(peer.port(), port);
        }
        // Linux without `CAP_NET_BIND_SERVICE` (and no lowered
        // `ip_unprivileged_port_start`) says no; macOS always lets a user
        // process bind one.
        other => {
            if cfg!(target_os = "macos") {
                panic!("macOS lets a user process bind a reserved port, got {other:?}");
            }
            assert!(peer.port() >= 1024);
        }
    }
}

#[tokio::test]
async fn an_ephemeral_connection_asks_for_no_reserved_port() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let conn = RpcConnection::connect(
        target,
        PROGRAM,
        Credential::None,
        SourcePort::Ephemeral,
        TransportOptions::PRODUCTION,
    )
    .await
    .unwrap();
    assert_eq!(conn.reserved_port(), ReservedPort::NotAsked);
}

#[tokio::test]
async fn ping_is_true_on_any_reply_and_false_on_a_refusal() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let serving = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = ServerConn::new(stream);
        let call = server.next_call().await.unwrap();
        assert_eq!((call.program, call.version, call.procedure), (100_003, 3, 0));
        assert_eq!(call.credential_flavor, 0);
        server.reply(call.xid, |_| {}).await;
    });
    assert!(ping(target, PROGRAM).await);
    serving.await.unwrap();

    // Nothing listens there any more.
    assert!(!ping(target, PROGRAM).await);
}

#[tokio::test]
async fn a_reset_tcp_connection_poisons_the_transport_immediately() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let serving = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let call = decode_call(Bytes::copy_from_slice(&buf[4..n])).unwrap();
        // Half a reply, then a reset.
        let reply = success_reply(call.xid, |enc| enc.u32(1));
        stream.write_all(&reply[..6]).await.unwrap();
        #[allow(
            deprecated,
            reason = "a zero linger is how a test sends a reset; it doesn't block on drop"
        )]
        stream.set_linger(Some(Duration::ZERO)).unwrap();
    });
    let conn = RpcConnection::connect(
        target,
        PROGRAM,
        Credential::None,
        SourcePort::Ephemeral,
        TransportOptions::PRODUCTION,
    )
    .await
    .unwrap();
    let answer = ask(&conn, 1, Repeat::Safe).await;
    serving.await.unwrap();
    assert!(matches!(answer, Err(CallError::Disconnected(_))), "{answer:?}");
}
