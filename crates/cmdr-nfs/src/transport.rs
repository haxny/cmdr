//! One TCP connection to one RPC program, with every call on it pipelined.
//!
//! A writer task sends whole records; a reader task reassembles each reply
//! (RFC 5531 § 11 record marking) and hands it to the call waiting on its XID.
//! Calls overlap freely: a READ never waits for the READ before it to answer.
//!
//! - ❗ **Cancelling a call forgets its XID**, ❌ never drops the connection:
//!   NFSv3 is stateless, so nothing on the server waits for the reply to be
//!   read, and a late one is dropped on arrival.
//! - ❗ **A framing error poisons the connection** (a bad record mark, a record
//!   past the cap, a reset mid-record): every waiting call answers
//!   [`CallError::Disconnected`] at once. A reply that frames fine but won't
//!   decode fails only its own call ([`CallError::Decode`]).
//! - ❗ **Silence goes through `cmdr_fs::volume::liveness`.** Every byte heard
//!   and every record the socket takes feeds it; while calls wait on a quiet
//!   server, the watchdog sends RPC `NULL` on a FRESH connection, and the
//!   ladder giving up poisons this one.
//! - ❗ **A reply that never comes.** A server may drop one request silently
//!   (Linux `nfsd` under memory pressure), and the replies to the calls around
//!   it keep the ladder fed. So a call unanswered for
//!   [`TransportTimings::stuck_after`] while a call sent AFTER it has been
//!   answered is resent once with the SAME XID (so the server's duplicate-request
//!   cache can answer it), but only when it's [`Repeat::Safe`]. A
//!   [`Repeat::Never`] call fails as [`CallError::Unanswered`] and retires the
//!   connection; a second silence on a resent call poisons it.
//! - ❗ **The reserved source port.** A `secure` export refuses a client whose
//!   source port is past 1023. macOS lets an unprivileged process bind one, so
//!   [`SourcePort::Reserved`] tries a random port in 600–1023 and walks down,
//!   and falls back to an ephemeral port (recording
//!   [`ReservedPort::Unavailable`]) where the OS says no.

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::liveness::{self, Liveness, Timings};
use log::debug;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::proto::rpc::{
    AcceptError, Credential, Procedure, Program, Rejection, Repeat, ReplyBody, decode_reply, encode_call_header,
    peek_xid,
};
use crate::xdr::{Decoder, Encoder, XdrError};

/// The record mark's high bit: this fragment is the record's last.
const LAST_FRAGMENT: u32 = 0x8000_0000;

/// The largest READ this client asks for, and so the largest payload a reply
/// carries.
pub(crate) const MAX_PAYLOAD: u32 = 1024 * 1024;

/// The largest reply record accepted: a [`MAX_PAYLOAD`] READ plus generous room
/// for its headers and attributes. ❗ A record past it poisons the connection
/// before a byte of it is buffered.
pub(crate) const DEFAULT_RECORD_CAP: usize = MAX_PAYLOAD as usize + 64 * 1024;

/// The lowest port [`SourcePort::Reserved`] tries. Below 600 sit the ports
/// well-known services own.
const RESERVED_LOW: u16 = 600;
/// The highest reserved port.
const RESERVED_HIGH: u16 = 1023;
/// How many reserved ports a dial tries before settling for an ephemeral one.
const RESERVED_TRIES: usize = 32;

/// How long the transport waits before it calls a server silent or a call stuck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportTimings {
    /// The silence ladder.
    pub(crate) liveness: Timings,
    /// How long a call waits, while calls sent after it are answered, before
    /// it counts as dropped by the server.
    pub(crate) stuck_after: Duration,
}

impl TransportTimings {
    /// NFS's own ladder: 5 s of silence, then two probes of 5 s each, so a dead
    /// server is gone after 15 s (the other backends allow 30 s). `NULL` is
    /// answered by the RPC layer even on a busy server, so a probe that goes
    /// unanswered says a lot more than an HTTP one. A call is stuck after 20 s,
    /// so a dropped metadata reply gets its one resend well inside the 60 s
    /// call budget.
    pub(crate) const PRODUCTION: Self = Self {
        liveness: Timings {
            quiet: Duration::from_secs(5),
            probe_budget: Duration::from_secs(5),
            unanswered_limit: 2,
        },
        stuck_after: Duration::from_secs(20),
    };
}

/// How a connection is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransportOptions {
    /// The largest reply record accepted, all fragments together.
    pub(crate) record_cap: usize,
    /// The silence ladder and the stuck-call rule.
    pub(crate) timings: TransportTimings,
}

impl TransportOptions {
    /// What every production connection uses.
    pub(crate) const PRODUCTION: Self = Self {
        record_cap: DEFAULT_RECORD_CAP,
        timings: TransportTimings::PRODUCTION,
    };
}

/// Which source port a connection dials from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourcePort {
    /// Below 1024, where the OS allows it: MOUNT and NFS, which a `secure`
    /// export checks.
    Reserved,
    /// Whatever the OS hands out: portmap and the liveness probe, which no
    /// server checks, so a ladder probing every 5 s doesn't burn the range.
    Ephemeral,
}

/// What came of asking for a reserved source port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReservedPort {
    /// Bound to this port, below 1024.
    Bound(u16),
    /// Asked for one and the OS said no (Linux without
    /// `CAP_NET_BIND_SERVICE`), or every port tried was taken: the connection
    /// dials from an ephemeral port, and a `secure` export will refuse it.
    Unavailable,
    /// The connection didn't ask for one ([`SourcePort::Ephemeral`], or a
    /// stream handed in from elsewhere).
    NotAsked,
}

/// Why a call didn't produce results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallError {
    /// The connection is gone: reset, closed, framed garbage, or a server the
    /// silence ladder gave up on. Carries the log-only reason.
    Disconnected(String),
    /// The caller's budget for this call ran out.
    TimedOut,
    /// A [`Repeat::Never`] call went unanswered while later calls were
    /// answered: the server likely dropped it, and resending could change the
    /// answer. The connection is retiring.
    Unanswered,
    /// The connection is retiring (an earlier call went unanswered) and takes
    /// no new calls. Dial a fresh one.
    Retired,
    /// The server answered `MSG_ACCEPTED` with an `accept_stat` other than
    /// `SUCCESS`.
    NotAccepted(AcceptError),
    /// The server answered `MSG_DENIED`.
    Denied(Rejection),
    /// The reply framed fine but wouldn't decode.
    Decode(XdrError),
}

impl From<XdrError> for CallError {
    fn from(err: XdrError) -> Self {
        Self::Decode(err)
    }
}

/// A one-shot "is the server there?" check, answering whether ANY reply came.
/// The silence ladder applies its own budget.
pub(crate) type Probe = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// Why the reader stopped.
#[derive(Debug)]
pub(crate) enum FrameError {
    /// The peer closed cleanly between records.
    Closed,
    /// The peer closed inside a record.
    ClosedMidRecord,
    /// The record grew past the cap.
    TooLarge {
        /// The record's length had this fragment been accepted.
        len: usize,
        /// The cap.
        cap: usize,
    },
    /// The socket failed.
    Io(io::ErrorKind),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(out, "the server closed the connection"),
            Self::ClosedMidRecord => write!(out, "the connection closed inside a reply"),
            Self::TooLarge { len, cap } => write!(out, "a reply length of {len}, past the cap of {cap}"),
            Self::Io(kind) => write!(out, "the connection failed: {kind}"),
        }
    }
}

/// Reads `buf.len()` bytes, or answers how many arrived before the peer closed.
async fn read_full<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut [u8]) -> Result<usize, FrameError> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]).await {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) => return Err(FrameError::Io(err.kind())),
        }
    }
    Ok(filled)
}

/// Reads one whole record, every fragment of it, calling `heard` on each piece
/// that arrives.
///
/// ❗ Each fragment's length is checked against what's left of `cap` BEFORE any
/// room is reserved for it, so a mark claiming 2 GiB costs four bytes.
pub(crate) async fn read_record<R: AsyncRead + Unpin>(
    reader: &mut R,
    cap: usize,
    heard: &(dyn Fn() + Sync),
) -> Result<Bytes, FrameError> {
    let mut record = BytesMut::new();
    let mut started = false;
    loop {
        let mut mark = [0u8; 4];
        match read_full(reader, &mut mark).await? {
            4 => {}
            0 if !started => return Err(FrameError::Closed),
            _ => return Err(FrameError::ClosedMidRecord),
        }
        started = true;
        heard();
        let mark = u32::from_be_bytes(mark);
        let len = (mark & !LAST_FRAGMENT) as usize;
        let total = record.len().saturating_add(len);
        if total > cap {
            return Err(FrameError::TooLarge { len: total, cap });
        }
        record.reserve(len);
        let mut fragment = (&mut *reader).take(len as u64);
        while fragment.limit() > 0 {
            match fragment.read_buf(&mut record).await {
                Ok(0) => return Err(FrameError::ClosedMidRecord),
                Ok(_) => heard(),
                Err(err) => return Err(FrameError::Io(err.kind())),
            }
        }
        if mark & LAST_FRAGMENT != 0 {
            return Ok(record.freeze());
        }
    }
}

/// Frames `body` as one single-fragment record: the mark, then the bytes.
pub(crate) fn frame_record(write_body: impl FnOnce(&mut Encoder)) -> Bytes {
    let mut buf = BytesMut::with_capacity(256);
    buf.put_u32(0);
    let mut enc = Encoder::wrap(buf);
    write_body(&mut enc);
    let mut buf = enc.finish();
    let len = u32::try_from(buf.len() - 4).unwrap_or(u32::MAX >> 1) & !LAST_FRAGMENT;
    buf[..4].copy_from_slice(&(len | LAST_FRAGMENT).to_be_bytes());
    buf.freeze()
}

/// A fresh, process-wide XID. Seeded randomly once, so XIDs don't repeat across
/// a reconnect (a server's duplicate-request cache would answer a new call
/// with an old reply) nor, in all likelihood, across a restart.
fn next_xid() -> u32 {
    static NEXT: OnceLock<AtomicU32> = OnceLock::new();
    NEXT.get_or_init(|| AtomicU32::new(random_u32()))
        .fetch_add(1, Ordering::Relaxed)
}

/// A random-enough `u32` with no dependency: std's per-process random hash
/// keys over the clock.
pub(crate) fn random_u32() -> u32 {
    use std::hash::{BuildHasher, RandomState};
    let hash = RandomState::new().hash_one((std::process::id(), std::time::SystemTime::now()));
    (hash ^ (hash >> 32)) as u32
}

/// One call waiting for its reply.
struct Pending {
    reply: oneshot::Sender<Bytes>,
    /// What was asked, for the log and the test tap.
    procedure: Procedure,
    /// Its place in send order, so a stuck call can tell whether calls sent
    /// after it have been answered.
    sent_as: u64,
}

struct Shared {
    credential: Credential,
    outgoing: mpsc::UnboundedSender<Bytes>,
    pending: Mutex<HashMap<u32, Pending>>,
    liveness: Arc<Liveness>,
    probe: Probe,
    timings: TransportTimings,
    /// Cancelled once: poisoned, closed, or given up on. Every waiting call
    /// races it.
    closed: CancellationToken,
    /// Why `closed` fired, for the log. The first reason wins.
    why_closed: OnceLock<String>,
    /// No new calls: an earlier one went unanswered.
    retiring: AtomicBool,
    /// Calls sent so far, the clock `Pending::sent_as` reads.
    sent: AtomicU64,
    /// The highest `sent_as` any reply has answered.
    answered_up_to: AtomicU64,
    reserved_port: ReservedPort,
}

impl Shared {
    fn poison(&self, why: String) {
        if self.why_closed.set(why).is_ok() {
            debug!(target: "nfs", "connection closed: {}", self.why_closed.get().map_or("", String::as_str));
        }
        self.closed.cancel();
        // Dropping each sender answers its waiter, which reads `why_closed`.
        self.pending.lock_ignore_poison().clear();
    }

    fn disconnected(&self) -> CallError {
        CallError::Disconnected(self.why_closed.get().cloned().unwrap_or_default())
    }

    fn deliver(&self, record: Bytes) {
        let Some(xid) = peek_xid(&record) else {
            debug!(target: "nfs", "dropped a {}-byte record too short to name its call", record.len());
            return;
        };
        let Some(pending) = self.pending.lock_ignore_poison().remove(&xid) else {
            // A call that was cancelled, or the second answer to a resent one.
            return;
        };
        self.answered_up_to.fetch_max(pending.sent_as, Ordering::SeqCst);
        #[cfg(test)]
        tap::record(pending.procedure, &record);
        let _ = pending.reply.send(record);
    }

    fn send(&self, record: Bytes) -> Result<(), CallError> {
        self.outgoing.send(record).map_err(|_| self.disconnected())
    }

    fn stamp(&self) -> u64 {
        self.sent.fetch_add(1, Ordering::SeqCst) + 1
    }
}

/// Forgets a call's XID when the call ends, however it ends.
struct Forget<'a> {
    shared: &'a Shared,
    xid: u32,
}

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        self.shared.pending.lock_ignore_poison().remove(&self.xid);
    }
}

/// One RPC connection. Cheap to clone; the connection closes when the last
/// clone drops (or on [`close`](Self::close)).
#[derive(Clone)]
pub(crate) struct RpcConnection {
    shared: Arc<Shared>,
    _close_on_last_drop: Arc<DropGuard>,
}

impl RpcConnection {
    /// Dials `target` and speaks to `program` there as `credential`.
    ///
    /// No budget of its own: the dial wraps it in one, per resolved address.
    pub(crate) async fn connect(
        target: SocketAddr,
        program: Program,
        credential: Credential,
        source: SourcePort,
        options: TransportOptions,
    ) -> io::Result<Self> {
        let (stream, reserved_port) = match source {
            SourcePort::Reserved => connect_from_reserved_port(target).await?,
            SourcePort::Ephemeral => (TcpStream::connect(target).await?, ReservedPort::NotAsked),
        };
        stream.set_nodelay(true)?;
        Ok(Self::over_stream(
            stream,
            credential,
            null_probe(target, program),
            reserved_port,
            options,
        ))
    }

    /// A connection over any byte stream: what [`connect`](Self::connect)
    /// builds on, and what an in-process fake server hands in. `probe` is the
    /// silence ladder's fresh-connection question. Spawns the reader and the
    /// writer onto the current runtime.
    pub(crate) fn over_stream<S>(
        stream: S,
        credential: Credential,
        probe: Probe,
        reserved_port: ReservedPort,
        options: TransportOptions,
    ) -> Self
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (reader, writer) = tokio::io::split(stream);
        let (outgoing, records) = mpsc::unbounded_channel();
        let closed = CancellationToken::new();
        let shared = Arc::new(Shared {
            credential,
            outgoing,
            pending: Mutex::new(HashMap::new()),
            liveness: Arc::new(Liveness::new()),
            probe,
            timings: options.timings,
            closed: closed.clone(),
            why_closed: OnceLock::new(),
            retiring: AtomicBool::new(false),
            sent: AtomicU64::new(0),
            answered_up_to: AtomicU64::new(0),
            reserved_port,
        });
        tokio::spawn(read_loop(reader, Arc::clone(&shared), options.record_cap));
        tokio::spawn(write_loop(writer, records, Arc::clone(&shared)));
        tokio::spawn(poison_when_lost(Arc::clone(&shared)));
        Self {
            shared,
            _close_on_last_drop: Arc::new(closed.drop_guard()),
        }
    }

    /// Sends one call and waits for its results.
    ///
    /// `args` writes the procedure's arguments after the header. `budget` caps
    /// the whole wait (`None`: silence is the only limit, the READ and WRITE
    /// shape). Dropping the future forgets the XID and nothing else.
    pub(crate) async fn call(
        &self,
        procedure: Procedure,
        args: impl FnOnce(&mut Encoder),
        budget: Option<Duration>,
    ) -> Result<Decoder, CallError> {
        let shared = &*self.shared;
        if shared.closed.is_cancelled() {
            return Err(shared.disconnected());
        }
        if shared.retiring.load(Ordering::SeqCst) {
            return Err(CallError::Retired);
        }
        let xid = next_xid();
        let record = frame_record(|enc| {
            encode_call_header(enc, xid, procedure, &shared.credential);
            args(enc);
        });
        let (reply_tx, mut reply_rx) = oneshot::channel();
        let mut sent_as = shared.stamp();
        shared.pending.lock_ignore_poison().insert(
            xid,
            Pending {
                reply: reply_tx,
                procedure,
                sent_as,
            },
        );
        let _forget = Forget { shared, xid };
        let waiting = shared.liveness.begin();
        if waiting.needs_a_watch() {
            let probe = Arc::clone(&shared.probe);
            tokio::spawn(liveness::watch(
                Arc::clone(&shared.liveness),
                shared.timings.liveness,
                "nfs",
                move || probe(),
            ));
        }
        shared.send(record.clone())?;

        let deadline = budget.map(|budget| Instant::now() + budget);
        let stuck_after = shared.timings.stuck_after;
        // After the first full wait, look again often: the call is overdue, and
        // the moment a later call's answer shows it was dropped is worth catching.
        let recheck = (stuck_after / 6).max(Duration::from_millis(1));
        let mut next_look = Instant::now() + stuck_after;
        let mut resent = false;
        loop {
            tokio::select! {
                // A reply that's here wins over a timer firing at the same instant.
                biased;
                reply = &mut reply_rx => {
                    let Ok(record) = reply else { return Err(shared.disconnected()) };
                    return match decode_reply(record)? {
                        (_, ReplyBody::Success(results)) => Ok(results),
                        (_, ReplyBody::NotAccepted(why)) => Err(CallError::NotAccepted(why)),
                        (_, ReplyBody::Denied(why)) => Err(CallError::Denied(why)),
                    };
                }
                () = shared.closed.cancelled() => return Err(shared.disconnected()),
                () = sleep_until(deadline) => return Err(CallError::TimedOut),
                () = tokio::time::sleep_until(next_look) => {
                    if shared.answered_up_to.load(Ordering::SeqCst) <= sent_as {
                        // Nothing sent after this call has been answered either:
                        // the server is slow or silent as a whole, which is the
                        // ladder's to judge.
                        next_look = Instant::now() + recheck;
                        continue;
                    }
                    match (procedure.repeat, resent) {
                        (Repeat::Safe, false) => {
                            debug!(target: "nfs", "procedure {} went unanswered while later calls were answered; resending it once", procedure.number);
                            sent_as = shared.stamp();
                            if let Some(pending) = shared.pending.lock_ignore_poison().get_mut(&xid) {
                                pending.sent_as = sent_as;
                            }
                            shared.send(record.clone())?;
                            resent = true;
                            next_look = Instant::now() + stuck_after;
                        }
                        (Repeat::Safe, true) => {
                            shared.poison(format!("procedure {} went unanswered twice", procedure.number));
                            return Err(shared.disconnected());
                        }
                        (Repeat::Never, _) => {
                            debug!(target: "nfs", "procedure {} went unanswered and can't be resent; retiring the connection", procedure.number);
                            shared.retiring.store(true, Ordering::SeqCst);
                            return Err(CallError::Unanswered);
                        }
                    }
                }
            }
        }
    }

    /// The silence evidence this connection feeds on every byte. A session
    /// that watches the same server shares it, so one ladder decides.
    pub(crate) fn liveness(&self) -> &Arc<Liveness> {
        &self.shared.liveness
    }

    /// What came of asking for a reserved source port.
    pub(crate) fn reserved_port(&self) -> ReservedPort {
        self.shared.reserved_port
    }

    /// Cancelled once the connection is unusable for good: poisoned, closed,
    /// or given up on by the silence ladder.
    pub(crate) fn lost(&self) -> CancellationToken {
        self.shared.closed.clone()
    }

    /// Whether a new call can go out on this connection. `false` once it's
    /// lost or retiring: dial a fresh one.
    pub(crate) fn is_usable(&self) -> bool {
        !self.shared.closed.is_cancelled() && !self.shared.retiring.load(Ordering::SeqCst)
    }

    /// Calls waiting for a reply right now.
    #[cfg(test)]
    pub(crate) fn waiting_calls(&self) -> usize {
        self.shared.pending.lock_ignore_poison().len()
    }

    /// Closes the connection now. Every waiting call answers
    /// [`CallError::Disconnected`].
    pub(crate) fn close(&self) {
        self.shared.poison("closed by this client".to_string());
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

async fn read_loop<R: AsyncRead + Unpin>(mut reader: R, shared: Arc<Shared>, cap: usize) {
    let liveness = Arc::clone(&shared.liveness);
    let heard = move || liveness.heard();
    let why = loop {
        let next = tokio::select! {
            next = read_record(&mut reader, cap, &heard) => next,
            () = shared.closed.cancelled() => return,
        };
        match next {
            Ok(record) => shared.deliver(record),
            Err(why) => break why,
        }
    };
    shared.poison(why.to_string());
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut records: mpsc::UnboundedReceiver<Bytes>,
    shared: Arc<Shared>,
) {
    loop {
        let record = tokio::select! {
            record = records.recv() => match record {
                Some(record) => record,
                None => break,
            },
            () = shared.closed.cancelled() => break,
        };
        let written = tokio::select! {
            written = writer.write_all(&record) => written,
            () = shared.closed.cancelled() => break,
        };
        match written {
            Ok(()) => shared.liveness.heard(),
            Err(err) => {
                shared.poison(format!("the connection failed while sending: {}", err.kind()));
                break;
            }
        }
    }
    let _ = writer.shutdown().await;
}

async fn poison_when_lost(shared: Arc<Shared>) {
    tokio::select! {
        () = shared.liveness.lost().cancelled() => shared.poison("the server went silent".to_string()),
        () = shared.closed.cancelled() => {}
    }
}

/// The silence ladder's question for a connection to `program` at `target`:
/// `NULL` on a FRESH connection from an ephemeral port, true on any reply.
fn null_probe(target: SocketAddr, program: Program) -> Probe {
    Arc::new(move || Box::pin(ping(target, program)))
}

/// RPC `NULL` to `program` at `target` on a fresh connection: true when
/// anything that frames as a reply comes back.
pub(crate) async fn ping(target: SocketAddr, program: Program) -> bool {
    let Ok(mut stream) = TcpStream::connect(target).await else {
        return false;
    };
    let xid = next_xid();
    let record = frame_record(|enc| {
        encode_call_header(
            enc,
            xid,
            Procedure {
                program,
                number: 0,
                repeat: Repeat::Safe,
            },
            &Credential::None,
        );
    });
    if stream.write_all(&record).await.is_err() {
        return false;
    }
    // A NULL reply is a few dozen bytes; anything framing past 4 KiB isn't one.
    match read_record(&mut stream, 4096, &|| {}).await {
        Ok(reply) => peek_xid(&reply) == Some(xid),
        Err(_) => false,
    }
}

/// Dials `target` from a port in 600–1023: a random start, then walking down
/// (wrapping at the bottom), up to [`RESERVED_TRIES`] ports.
///
/// - `SO_REUSEADDR` is on, or a reconnect inside `TIME_WAIT` would find every
///   recently used port refused.
/// - `EADDRINUSE` moves on to the next port, at `bind` AND at `connect`: with
///   `SO_REUSEADDR` the clash with a `TIME_WAIT` 4-tuple surfaces at `connect`
///   (as `EADDRINUSE` or `EADDRNOTAVAIL`).
/// - `EACCES` / `EPERM` (Linux without `CAP_NET_BIND_SERVICE`) and running out
///   of tries both fall back to an ephemeral port, recorded as
///   [`ReservedPort::Unavailable`] so a `secure` export's refusal can say why.
async fn connect_from_reserved_port(target: SocketAddr) -> io::Result<(TcpStream, ReservedPort)> {
    let span = u32::from(RESERVED_HIGH - RESERVED_LOW) + 1;
    let mut port = RESERVED_LOW + (random_u32() % span) as u16;
    let unspecified = match target.ip() {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    for _ in 0..RESERVED_TRIES {
        let socket = match target {
            SocketAddr::V4(_) => TcpSocket::new_v4()?,
            SocketAddr::V6(_) => TcpSocket::new_v6()?,
        };
        socket.set_reuseaddr(true)?;
        match socket.bind(SocketAddr::new(unspecified, port)) {
            Ok(()) => match socket.connect(target).await {
                Ok(stream) => return Ok((stream, ReservedPort::Bound(port))),
                Err(err) if is_port_taken(&err) => {}
                Err(err) => return Err(err),
            },
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
                debug!(target: "nfs", "this process may not bind a port below 1024; dialing from an ephemeral one");
                return Ok((TcpStream::connect(target).await?, ReservedPort::Unavailable));
            }
            Err(err) if is_port_taken(&err) => {}
            Err(err) => return Err(err),
        }
        port = if port == RESERVED_LOW { RESERVED_HIGH } else { port - 1 };
    }
    debug!(target: "nfs", "every reserved port tried was taken; dialing from an ephemeral one");
    Ok((TcpStream::connect(target).await?, ReservedPort::Unavailable))
}

fn is_port_taken(err: &io::Error) -> bool {
    matches!(err.kind(), io::ErrorKind::AddrInUse | io::ErrorKind::AddrNotAvailable)
}

/// A test-only record of every reply delivered, process-wide: how the fixture
/// cells capture real server replies as decoding fixtures.
#[cfg(test)]
pub(crate) mod tap {
    use std::sync::Mutex;

    use bytes::Bytes;
    use cmdr_fs::ignore_poison::IgnorePoison;

    use crate::proto::rpc::Procedure;

    static TAPPED: Mutex<Option<Vec<(Procedure, Bytes)>>> = Mutex::new(None);

    /// Starts recording every reply this process's connections deliver.
    pub(crate) fn start() {
        *TAPPED.lock_ignore_poison() = Some(Vec::new());
    }

    /// Stops recording and hands back what was recorded, in order.
    pub(crate) fn take() -> Vec<(Procedure, Bytes)> {
        TAPPED.lock_ignore_poison().take().unwrap_or_default()
    }

    pub(super) fn record(procedure: Procedure, record: &Bytes) {
        if let Some(tapped) = TAPPED.lock_ignore_poison().as_mut() {
            tapped.push((procedure, record.clone()));
        }
    }
}

#[cfg(test)]
#[path = "transport_test.rs"]
mod transport_test;
