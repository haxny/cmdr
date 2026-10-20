//! An in-process NFSv3 server: portmap, MOUNT, and every NFS procedure the
//! volume speaks, over an in-memory tree, with knobs for what a real server
//! does only sometimes. The twin of `cmdr-webdav`'s `fake_dav.rs`.
//!
//! Why it exists beside the knfsd fixture: the fixture is the truth for what a
//! NAS answers, but it can't be told to go silent, drop one reply, answer out
//! of order, rotate its write verifier, or refuse READDIRPLUS, and it can't run
//! on a paused clock. This can, and the cells that need any of those use it.
//!
//! Two ways in:
//!
//! - **Over TCP** ([`FakeNfs::start_with`]): one listener answers all three
//!   programs, so `GETPORT` names the listener's own port for MOUNT and NFS,
//!   and the real dial (`dial::mount`) runs unchanged against
//!   [`FakeNfs::server`].
//! - **Over an in-memory pipe** ([`FakeNfs::duplex_dialer`]): no socket, so a
//!   cell on a paused clock is exact. The pipe's "peer port" is 900, a
//!   reserved one.
//!
//! Every knob is safe to flip while calls are in flight.

#![allow(
    dead_code,
    reason = "the write and outage cells drive the knobs the read side doesn't: verifier, short writes, resets, drops"
)]

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::reconnect::{Dialer, NfsClient, Resolved};
use crate::dial::{DialOptions, credential_for};
use crate::errors::NfsConnectError;
use crate::params::{NfsConnectionParams, NfsServer};
use crate::proto::nfs3::{Fattr3, FileHandle, FileType, Nfs3Client, NfsTime};
use crate::proto::server::nfs3_wire;
use crate::proto::server::{CallFrame, decode_call, success_reply};
use crate::transport::{DEFAULT_RECORD_CAP, Probe, ReservedPort, RpcConnection, read_record};
use crate::xdr::{Decoder, Encoder};

/// The one export the fake serves.
pub(super) const FAKE_EXPORT: &str = "/export";

const PORTMAP: u32 = 100_000;
const MOUNT: u32 = 100_005;
const NFS: u32 = 100_003;

/// `nfsstat3` numbers the fake answers with.
mod status {
    pub(super) const PERM: u32 = 1;
    pub(super) const NOENT: u32 = 2;
    pub(super) const ACCES: u32 = 13;
    pub(super) const EXIST: u32 = 17;
    pub(super) const NOTDIR: u32 = 20;
    pub(super) const ISDIR: u32 = 21;
    pub(super) const INVAL: u32 = 22;
    pub(super) const NOTEMPTY: u32 = 66;
    pub(super) const STALE: u32 = 70;
    pub(super) const BADHANDLE: u32 = 10001;
    pub(super) const NOTSUPP: u32 = 10004;
}

/// NFS procedure numbers, for the per-procedure knobs and counters.
pub(super) mod proc {
    pub(crate) const NULL: u32 = 0;
    pub(crate) const GETATTR: u32 = 1;
    pub(crate) const SETATTR: u32 = 2;
    pub(crate) const LOOKUP: u32 = 3;
    pub(crate) const ACCESS: u32 = 4;
    pub(crate) const READLINK: u32 = 5;
    pub(crate) const READ: u32 = 6;
    pub(crate) const WRITE: u32 = 7;
    pub(crate) const CREATE: u32 = 8;
    pub(crate) const MKDIR: u32 = 9;
    pub(crate) const REMOVE: u32 = 12;
    pub(crate) const RMDIR: u32 = 13;
    pub(crate) const RENAME: u32 = 14;
    pub(crate) const READDIR: u32 = 16;
    pub(crate) const READDIRPLUS: u32 = 17;
    pub(crate) const FSSTAT: u32 = 18;
    pub(crate) const FSINFO: u32 = 19;
    pub(crate) const PATHCONF: u32 = 20;
    pub(crate) const COMMIT: u32 = 21;
}

#[derive(Debug, Clone)]
enum Body {
    Dir(BTreeMap<Vec<u8>, u64>),
    File(Vec<u8>),
    Link(Vec<u8>),
}

#[derive(Debug, Clone)]
struct Node {
    /// Bumped when the node is "replaced": every handle minted before answers
    /// `STALE` from then on.
    generation: u32,
    body: Body,
    mode: u32,
    uid: u32,
    gid: u32,
    atime: NfsTime,
    mtime: NfsTime,
    ctime: NfsTime,
}

struct Tree {
    nodes: HashMap<u64, Node>,
    next_id: u64,
    /// The fake's clock, in whole seconds: every change ticks it, so a stamp
    /// always moves.
    clock: u32,
}

const ROOT: u64 = 1;

impl Tree {
    fn new() -> Self {
        let mut tree = Self {
            nodes: HashMap::new(),
            next_id: ROOT + 1,
            clock: 1_700_000_000,
        };
        let now = tree.now();
        tree.nodes.insert(
            ROOT,
            Node {
                generation: 1,
                body: Body::Dir(BTreeMap::new()),
                mode: 0o777,
                uid: 0,
                gid: 0,
                atime: now,
                mtime: now,
                ctime: now,
            },
        );
        tree
    }

    fn now(&self) -> NfsTime {
        NfsTime {
            seconds: self.clock,
            nseconds: 0,
        }
    }

    fn tick(&mut self) -> NfsTime {
        self.clock += 1;
        self.now()
    }

    fn handle(&self, id: u64) -> FileHandle {
        let generation = self.nodes.get(&id).map_or(0, |node| node.generation);
        let mut bytes = Vec::with_capacity(12);
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.extend_from_slice(&generation.to_be_bytes());
        FileHandle(Bytes::from(bytes))
    }

    /// The node a handle names, or the status a real server would answer.
    fn node_of(&self, handle: &[u8]) -> Result<u64, u32> {
        if handle.len() != 12 {
            return Err(status::BADHANDLE);
        }
        let id = u64::from_be_bytes(handle[..8].try_into().unwrap_or_default());
        let generation = u32::from_be_bytes(handle[8..].try_into().unwrap_or_default());
        match self.nodes.get(&id) {
            Some(node) if node.generation == generation => Ok(id),
            _ => Err(status::STALE),
        }
    }

    fn attrs(&self, id: u64) -> Fattr3 {
        let node = &self.nodes[&id];
        let (file_type, size) = match &node.body {
            Body::Dir(children) => (FileType::Directory, 4096 + children.len() as u64),
            Body::File(data) => (FileType::Regular, data.len() as u64),
            Body::Link(target) => (FileType::Symlink, target.len() as u64),
        };
        Fattr3 {
            file_type,
            mode: node.mode,
            nlink: if file_type == FileType::Directory { 2 } else { 1 },
            uid: node.uid,
            gid: node.gid,
            size,
            used: size,
            rdev: (0, 0),
            fsid: 7,
            fileid: id,
            atime: node.atime,
            mtime: node.mtime,
            ctime: node.ctime,
        }
    }

    fn children(&self, id: u64) -> Option<&BTreeMap<Vec<u8>, u64>> {
        match &self.nodes.get(&id)?.body {
            Body::Dir(children) => Some(children),
            _ => None,
        }
    }

    fn child(&self, dir: u64, name: &[u8]) -> Option<u64> {
        self.children(dir)?.get(name).copied()
    }

    fn by_path(&self, path: &str) -> Option<u64> {
        let mut at = ROOT;
        for part in path.split('/').filter(|part| !part.is_empty()) {
            at = self.child(at, part.as_bytes())?;
        }
        Some(at)
    }

    fn split(path: &str) -> (&str, &str) {
        let path = path.trim_end_matches('/');
        match path.rfind('/') {
            Some(cut) => (if cut == 0 { "/" } else { &path[..cut] }, &path[cut + 1..]),
            None => ("/", path),
        }
    }

    /// Adds `name` under `dir`, ticking the folder's dates.
    fn insert(&mut self, dir: u64, name: &[u8], body: Body, mode: u32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let now = self.tick();
        self.nodes.insert(
            id,
            Node {
                generation: 1,
                body,
                mode,
                uid: 1000,
                gid: 1000,
                atime: now,
                mtime: now,
                ctime: now,
            },
        );
        self.link(dir, name, id, now);
        id
    }

    fn link(&mut self, dir: u64, name: &[u8], id: u64, now: NfsTime) {
        if let Some(node) = self.nodes.get_mut(&dir) {
            if let Body::Dir(children) = &mut node.body {
                children.insert(name.to_vec(), id);
            }
            node.mtime = now;
            node.ctime = now;
        }
    }

    fn unlink(&mut self, dir: u64, name: &[u8]) -> Option<u64> {
        let now = self.tick();
        let node = self.nodes.get_mut(&dir)?;
        let Body::Dir(children) = &mut node.body else {
            return None;
        };
        let id = children.remove(name)?;
        node.mtime = now;
        node.ctime = now;
        Some(id)
    }

    fn wcc_before(&self, id: u64) -> (u64, NfsTime, NfsTime) {
        let attrs = self.attrs(id);
        (attrs.size, attrs.mtime, attrs.ctime)
    }
}

/// What the knobs make the fake do, beyond answering.
#[derive(Default)]
struct Knobs {
    no_readdirplus: AtomicBool,
    silent: AtomicBool,
    refuse_dials: AtomicBool,
    reorder_reads: AtomicBool,
    require_reserved_port: AtomicBool,
    /// 0: none. Otherwise a READ answers at most this many bytes.
    short_reads: AtomicU32,
    /// 0: none. Otherwise a WRITE stores and answers at most this many bytes.
    short_writes: AtomicU32,
    /// Entries per READDIR(PLUS) page.
    page_entries: AtomicU32,
    /// `rtpref` / `wtpref` FSINFO answers.
    transfer_size: AtomicU32,
    verifier: AtomicU64,
    /// One reply to this procedure (+1; 0 none) goes unsent.
    drop_one: AtomicU32,
    /// One reply to this procedure (+1; 0 none) is cut halfway, and the
    /// connection closes.
    reset_one: AtomicU32,
    /// Every NFS call by procedure number.
    calls: Mutex<HashMap<u32, usize>>,
}

/// The fake server. Cheap to clone; every clone is the same server.
#[derive(Clone)]
pub(super) struct FakeNfs {
    tree: Arc<Mutex<Tree>>,
    knobs: Arc<Knobs>,
    port: u16,
    stop: CancellationToken,
}

impl FakeNfs {
    /// A fake with an empty export and no listener: reach it through
    /// [`duplex_dialer`](Self::duplex_dialer).
    pub(super) fn new() -> Self {
        let knobs = Knobs::default();
        knobs.page_entries.store(64, Ordering::SeqCst);
        knobs.transfer_size.store(64 * 1024, Ordering::SeqCst);
        knobs.verifier.store(1, Ordering::SeqCst);
        Self {
            tree: Arc::new(Mutex::new(Tree::new())),
            knobs: Arc::new(knobs),
            port: 0,
            stop: CancellationToken::new(),
        }
    }

    /// `fake`, now listening on loopback, all three programs on one port.
    pub(super) async fn start_with(mut fake: Self) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a loopback port");
        fake.port = listener.local_addr().expect("a bound port").port();
        let served = fake.clone();
        tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    accepted = listener.accept() => accepted,
                    () = served.stop.cancelled() => return,
                };
                let Ok((stream, peer)) = accepted else { return };
                if served.knobs.refuse_dials.load(Ordering::SeqCst) {
                    drop(stream);
                    continue;
                }
                tokio::spawn(served.clone().serve(stream, peer.port()));
            }
        });
        fake
    }

    /// Where the TCP fake answers, as the dial wants it.
    pub(super) fn server(&self) -> NfsServer {
        NfsServer {
            host: "127.0.0.1".to_string(),
            port: None,
            portmap_port: Some(self.port),
        }
    }

    /// Params for the fake's export.
    pub(super) fn params(&self) -> NfsConnectionParams {
        NfsConnectionParams::new(
            self.server(),
            FAKE_EXPORT,
            crate::params::NfsIdentity { uid: 1000, gid: 1000 },
        )
    }

    /// Stops the listener. Connections already open keep being answered.
    pub(super) fn stop_listening(&self) {
        self.stop.cancel();
    }

    // ── The tree, as another client would change it ──────────────────

    pub(super) fn add_dir(&self, path: &str) {
        let mut tree = self.tree.lock_ignore_poison();
        let (parent, name) = Tree::split(path);
        let dir = tree.by_path(parent).expect("the parent exists");
        tree.insert(dir, name.as_bytes(), Body::Dir(BTreeMap::new()), 0o755);
    }

    pub(super) fn add_file(&self, path: &str, data: &[u8]) {
        let mut tree = self.tree.lock_ignore_poison();
        let (parent, name) = Tree::split(path);
        let dir = tree.by_path(parent).expect("the parent exists");
        tree.insert(dir, name.as_bytes(), Body::File(data.to_vec()), 0o644);
    }

    /// A name whose bytes aren't UTF-8.
    pub(super) fn add_file_named(&self, parent: &str, name: &[u8], data: &[u8]) {
        let mut tree = self.tree.lock_ignore_poison();
        let dir = tree.by_path(parent).expect("the parent exists");
        tree.insert(dir, name, Body::File(data.to_vec()), 0o644);
    }

    pub(super) fn add_link(&self, path: &str, target: &str) {
        let mut tree = self.tree.lock_ignore_poison();
        let (parent, name) = Tree::split(path);
        let dir = tree.by_path(parent).expect("the parent exists");
        tree.insert(dir, name.as_bytes(), Body::Link(target.as_bytes().to_vec()), 0o777);
    }

    pub(super) fn set_owner(&self, path: &str, uid: u32, gid: u32) {
        let mut tree = self.tree.lock_ignore_poison();
        let id = tree.by_path(path).expect("the path exists");
        let node = tree.nodes.get_mut(&id).expect("a node");
        node.uid = uid;
        node.gid = gid;
    }

    /// Ticks `path`'s `mtime` and `ctime`, as a change inside it would.
    pub(super) fn touch(&self, path: &str) {
        let mut tree = self.tree.lock_ignore_poison();
        let now = tree.tick();
        let id = tree.by_path(path).expect("the path exists");
        let node = tree.nodes.get_mut(&id).expect("a node");
        node.mtime = now;
        node.ctime = now;
    }

    /// "Replaces" `path` in place: same name and contents, and every handle
    /// minted for it before now answers `STALE`.
    pub(super) fn make_stale(&self, path: &str) {
        let mut tree = self.tree.lock_ignore_poison();
        let id = tree.by_path(path).expect("the path exists");
        tree.nodes.get_mut(&id).expect("a node").generation += 1;
    }

    /// Renames `from` to `to` as ANOTHER client would: handles keep working,
    /// and now name the new place.
    pub(super) fn rename(&self, from: &str, to: &str) {
        let mut tree = self.tree.lock_ignore_poison();
        let (from_parent, from_name) = Tree::split(from);
        let (to_parent, to_name) = Tree::split(to);
        let from_dir = tree.by_path(from_parent).expect("the source's folder");
        let to_dir = tree.by_path(to_parent).expect("the target's folder");
        let id = tree.unlink(from_dir, from_name.as_bytes()).expect("the source");
        let now = tree.tick();
        tree.link(to_dir, to_name.as_bytes(), id, now);
    }

    /// What `path` holds, if it's a file.
    pub(super) fn contents(&self, path: &str) -> Option<Vec<u8>> {
        let tree = self.tree.lock_ignore_poison();
        match &tree.nodes.get(&tree.by_path(path)?)?.body {
            Body::File(data) => Some(data.clone()),
            _ => None,
        }
    }

    /// Whether anything is at `path`.
    pub(super) fn exists(&self, path: &str) -> bool {
        self.tree.lock_ignore_poison().by_path(path).is_some()
    }

    // ── Knobs ────────────────────────────────────────────────────────

    /// Answers READDIRPLUS with `NFS3ERR_NOTSUPP` (unfs3's shape), so the
    /// client falls back to READDIR + LOOKUP.
    pub(super) fn refuse_readdirplus(&self, refuse: bool) {
        self.knobs.no_readdirplus.store(refuse, Ordering::SeqCst);
    }

    /// Reads every call and answers none, on every connection, the `NULL`
    /// probe's fresh one included: a NAS asleep, a cable pulled.
    pub(super) fn go_silent(&self, silent: bool) {
        self.knobs.silent.store(silent, Ordering::SeqCst);
    }

    /// New connections are closed at once (a stopped `nfsd`), through both
    /// the listener and the duplex dialer.
    pub(super) fn refuse_dials(&self, refuse: bool) {
        self.knobs.refuse_dials.store(refuse, Ordering::SeqCst);
    }

    /// READ replies go out in batches, last first.
    pub(super) fn reorder_reads(&self, reorder: bool) {
        self.knobs.reorder_reads.store(reorder, Ordering::SeqCst);
    }

    /// Refuses a client whose source port is past 1023: `MNT3ERR_ACCES` on
    /// `MNT`, `NFS3ERR_PERM` on every NFS call but `NULL` (knfsd's shape,
    /// measured: `apps/desktop/test/nfs-servers/README.md` § "Reaching
    /// /secure").
    pub(super) fn require_reserved_port(&self, require: bool) {
        self.knobs.require_reserved_port.store(require, Ordering::SeqCst);
    }

    /// Every READ answers at most `bytes` (0: as asked).
    pub(super) fn answer_reads_short(&self, bytes: u32) {
        self.knobs.short_reads.store(bytes, Ordering::SeqCst);
    }

    /// Every WRITE stores and answers at most `bytes` (0: all of it).
    pub(super) fn answer_writes_short(&self, bytes: u32) {
        self.knobs.short_writes.store(bytes, Ordering::SeqCst);
    }

    /// How many entries a listing page holds.
    pub(super) fn set_page_entries(&self, entries: u32) {
        self.knobs.page_entries.store(entries.max(1), Ordering::SeqCst);
    }

    /// The `rtpref` / `wtpref` FSINFO answers.
    pub(super) fn set_transfer_size(&self, bytes: u32) {
        self.knobs.transfer_size.store(bytes, Ordering::SeqCst);
    }

    /// A server reboot as WRITE and COMMIT see it: a new verifier, so data not
    /// yet committed counts as lost.
    pub(super) fn rotate_verifier(&self) {
        self.knobs.verifier.fetch_add(1, Ordering::SeqCst);
    }

    /// The next reply to NFS procedure `procedure` goes unsent (the call
    /// itself still happened).
    pub(super) fn drop_one_reply(&self, procedure: u32) {
        self.knobs.drop_one.store(procedure + 1, Ordering::SeqCst);
    }

    /// The next reply to NFS procedure `procedure` is cut halfway, and that
    /// connection closes.
    pub(super) fn reset_mid_reply(&self, procedure: u32) {
        self.knobs.reset_one.store(procedure + 1, Ordering::SeqCst);
    }

    /// How many NFS calls of `procedure` arrived.
    pub(super) fn calls(&self, procedure: u32) -> usize {
        self.knobs
            .calls
            .lock_ignore_poison()
            .get(&procedure)
            .copied()
            .unwrap_or(0)
    }

    /// Every NFS call that arrived, `NULL` included.
    pub(super) fn total_calls(&self) -> usize {
        self.knobs.calls.lock_ignore_poison().values().sum()
    }

    // ── The duplex way in ────────────────────────────────────────────

    /// A dialer that reaches this fake over an in-memory pipe, with no socket:
    /// what a cell on a paused clock dials through. Each dial is a fresh pipe
    /// from "port 900", a FSINFO, and the export's root handle.
    pub(super) fn duplex_dialer(&self, options: DialOptions) -> Dialer {
        let fake = self.clone();
        Dialer::scripted(
            options,
            move |params: NfsConnectionParams, _cancel: CancellationToken| {
                let fake = fake.clone();
                Box::pin(async move {
                    if fake.knobs.refuse_dials.load(Ordering::SeqCst) {
                        return Err(NfsConnectError::Unreachable("refused".to_string()));
                    }
                    let client = fake.duplex_client(&params, options);
                    let fsinfo = client
                        .nfs
                        .fsinfo(&client.root)
                        .await
                        .map_err(|err| NfsConnectError::Transport(format!("{err:?}")))?;
                    let client = NfsClient::over_connection(client.nfs, client.root, fsinfo, fake.probe());
                    Ok((
                        client,
                        Resolved {
                            export: FAKE_EXPORT.to_string(),
                            folder: None,
                        },
                    ))
                })
            },
        )
    }

    /// The silence ladder's question, answered the way a fresh connection to
    /// this fake would: at once, unless it's silent or refusing.
    fn probe(&self) -> Probe {
        let knobs = Arc::clone(&self.knobs);
        Arc::new(move || {
            let knobs = Arc::clone(&knobs);
            Box::pin(async move {
                if knobs.silent.load(Ordering::SeqCst) {
                    std::future::pending::<()>().await;
                }
                !knobs.refuse_dials.load(Ordering::SeqCst)
            })
        })
    }

    fn duplex_client(&self, params: &NfsConnectionParams, options: DialOptions) -> NfsClient {
        let (near, far) = tokio::io::duplex(4 << 20);
        tokio::spawn(self.clone().serve(far, 900));
        let conn = RpcConnection::over_stream(
            near,
            credential_for(params.identity),
            self.probe(),
            ReservedPort::Bound(900),
            options.transport,
        );
        let root = self.tree.lock_ignore_poison().handle(ROOT);
        NfsClient::over_connection(
            Nfs3Client::new(conn, Some(options.call_budget)),
            root,
            fake_fsinfo(64 * 1024),
            self.probe(),
        )
    }

    // ── Serving one connection ────────────────────────────────────────

    async fn serve<S: AsyncRead + AsyncWrite + Send + 'static>(self, stream: S, peer_port: u16) {
        let (mut reader, writer) = tokio::io::split(stream);
        let (out, outgoing) = mpsc::unbounded_channel::<Outgoing>();
        tokio::spawn(write_replies(writer, outgoing, Arc::clone(&self.knobs)));
        while let Ok(record) = read_record(&mut reader, DEFAULT_RECORD_CAP, &|| {}).await {
            let Ok(mut call) = decode_call(record) else { return };
            if call.program == NFS {
                *self.knobs.calls.lock_ignore_poison().entry(call.procedure).or_default() += 1;
            }
            if self.knobs.silent.load(Ordering::SeqCst) {
                continue;
            }
            let procedure = call.procedure;
            let is_nfs = call.program == NFS;
            let reply = self.answer(&mut call, peer_port);
            if is_nfs && take_once(&self.knobs.drop_one, procedure) {
                continue;
            }
            let sent = if is_nfs && take_once(&self.knobs.reset_one, procedure) {
                out.send(Outgoing::CutAndClose(reply))
            } else {
                out.send(Outgoing::Reply {
                    bytes: reply,
                    is_read: is_nfs && procedure == proc::READ,
                })
            };
            if sent.is_err() {
                return;
            }
        }
    }

    fn answer(&self, call: &mut CallFrame, peer_port: u16) -> Bytes {
        let xid = call.xid;
        let reserved = peer_port < 1024 || !self.knobs.require_reserved_port.load(Ordering::SeqCst);
        match (call.program, call.procedure) {
            (_, proc::NULL) => success_reply(xid, |_| {}),
            (PORTMAP, 3) => {
                let program = call.args.u32().unwrap_or(0);
                let port = if program == MOUNT || program == NFS {
                    u32::from(self.port)
                } else {
                    0
                };
                success_reply(xid, |enc| enc.u32(port))
            }
            (MOUNT, 1) => {
                let path = call.args.opaque(1024).unwrap_or_default();
                let tree = self.tree.lock_ignore_poison();
                success_reply(xid, |enc| {
                    if !reserved {
                        enc.u32(status::ACCES);
                    } else if &path[..] == FAKE_EXPORT.as_bytes() {
                        enc.u32(0);
                        nfs3_wire::handle(enc, &tree.handle(ROOT));
                        enc.u32(1);
                        enc.u32(1);
                    } else {
                        enc.u32(2);
                    }
                })
            }
            (MOUNT, 3) => success_reply(xid, |_| {}),
            (MOUNT, 5) => success_reply(xid, |enc| {
                enc.bool(true);
                enc.opaque(FAKE_EXPORT.as_bytes());
                enc.bool(false);
                enc.bool(false);
            }),
            (NFS, _) if !reserved => success_reply(xid, |enc| enc.u32(status::PERM)),
            (NFS, procedure) => self.answer_nfs(xid, procedure, &mut call.args),
            _ => crate::proto::server::not_accepted_reply(xid, 1, None),
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per procedure reads better as one table than split across helpers"
    )]
    fn answer_nfs(&self, xid: u32, procedure: u32, args: &mut Decoder) -> Bytes {
        let mut tree = self.tree.lock_ignore_poison();
        let knobs = &self.knobs;
        let outcome: Result<Results, u32> = (|| {
            let target = tree.node_of(&args.opaque(64).map_err(|_| status::BADHANDLE)?)?;
            match procedure {
                proc::GETATTR => {
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| nfs3_wire::fattr(enc, &attrs)))
                }
                proc::SETATTR => {
                    let before = tree.wcc_before(target);
                    let sattr = decode_sattr(args);
                    let now = tree.tick();
                    let node = tree.nodes.get_mut(&target).expect("a node");
                    if let Some(mode) = sattr.mode {
                        node.mode = mode;
                    }
                    if let (Some(size), Body::File(data)) = (sattr.size, &mut node.body) {
                        data.resize(usize::try_from(size).unwrap_or(0), 0);
                    }
                    node.atime = sattr.atime.unwrap_or(node.atime);
                    node.mtime = sattr.mtime.unwrap_or(node.mtime);
                    node.ctime = now;
                    let after = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| wcc(enc, before, &after)))
                }
                proc::LOOKUP => {
                    let name = args.opaque(255).map_err(|_| status::INVAL)?;
                    if tree.children(target).is_none() {
                        return Err(status::NOTDIR);
                    }
                    let child = tree.child(target, &name).ok_or(status::NOENT)?;
                    let (handle, attrs, dir) = (tree.handle(child), tree.attrs(child), tree.attrs(target));
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::handle(enc, &handle);
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        nfs3_wire::post_op_attr(enc, Some(&dir));
                    }))
                }
                proc::ACCESS => {
                    let asked = args.u32().unwrap_or(0);
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.u32(asked);
                    }))
                }
                proc::READLINK => {
                    let Body::Link(link) = &tree.nodes[&target].body else {
                        return Err(status::INVAL);
                    };
                    let (link, attrs) = (link.clone(), tree.attrs(target));
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.opaque(&link);
                    }))
                }
                proc::READ => {
                    let offset = args.u64().unwrap_or(0) as usize;
                    let mut count = args.u32().unwrap_or(0) as usize;
                    let short = knobs.short_reads.load(Ordering::SeqCst) as usize;
                    if short > 0 {
                        count = count.min(short);
                    }
                    let Body::File(data) = &tree.nodes[&target].body else {
                        return Err(status::ISDIR);
                    };
                    let from = offset.min(data.len());
                    let to = (from + count).min(data.len());
                    let (slice, eof) = (data[from..to].to_vec(), to == data.len());
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.u32(slice.len() as u32);
                        enc.bool(eof);
                        enc.opaque(&slice);
                    }))
                }
                proc::WRITE => {
                    let offset = args.u64().unwrap_or(0) as usize;
                    let _count = args.u32().unwrap_or(0);
                    let stable = args.u32().unwrap_or(0);
                    let mut data = args.opaque(usize::MAX).unwrap_or_default().to_vec();
                    let short = knobs.short_writes.load(Ordering::SeqCst) as usize;
                    if short > 0 {
                        data.truncate(short);
                    }
                    let before = tree.wcc_before(target);
                    let now = tree.tick();
                    let node = tree.nodes.get_mut(&target).expect("a node");
                    let Body::File(file) = &mut node.body else {
                        return Err(status::ISDIR);
                    };
                    if file.len() < offset + data.len() {
                        file.resize(offset + data.len(), 0);
                    }
                    file[offset..offset + data.len()].copy_from_slice(&data);
                    node.mtime = now;
                    node.ctime = now;
                    let after = tree.attrs(target);
                    let verifier = knobs.verifier.load(Ordering::SeqCst).to_be_bytes();
                    let written = data.len() as u32;
                    Ok(boxed(move |enc: &mut Encoder| {
                        wcc(enc, before, &after);
                        enc.u32(written);
                        enc.u32(stable);
                        enc.fixed_opaque(&verifier);
                    }))
                }
                proc::COMMIT => {
                    let before = tree.wcc_before(target);
                    let after = tree.attrs(target);
                    let verifier = knobs.verifier.load(Ordering::SeqCst).to_be_bytes();
                    Ok(boxed(move |enc: &mut Encoder| {
                        wcc(enc, before, &after);
                        enc.fixed_opaque(&verifier);
                    }))
                }
                proc::CREATE | proc::MKDIR => {
                    let name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let guarded = if procedure == proc::CREATE {
                        let how = args.u32().unwrap_or(0);
                        if how != 2 {
                            let _ = decode_sattr(args);
                        }
                        how != 0
                    } else {
                        let _ = decode_sattr(args);
                        true
                    };
                    if tree.children(target).is_none() {
                        return Err(status::NOTDIR);
                    }
                    let before = tree.wcc_before(target);
                    let id = match tree.child(target, &name) {
                        Some(_) if guarded => return Err(status::EXIST),
                        Some(existing) => {
                            if let Body::File(data) = &mut tree.nodes.get_mut(&existing).expect("a node").body {
                                data.clear();
                            }
                            existing
                        }
                        None if procedure == proc::MKDIR => {
                            tree.insert(target, &name, Body::Dir(BTreeMap::new()), 0o755)
                        }
                        None => tree.insert(target, &name, Body::File(Vec::new()), 0o644),
                    };
                    let (handle, attrs, after) = (tree.handle(id), tree.attrs(id), tree.attrs(target));
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_fh(enc, Some(&handle));
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        wcc(enc, before, &after);
                    }))
                }
                proc::REMOVE | proc::RMDIR => {
                    let name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let child = tree.child(target, &name).ok_or(status::NOENT)?;
                    match (&tree.nodes[&child].body, procedure) {
                        (Body::Dir(_), proc::REMOVE) => return Err(status::ISDIR),
                        (Body::Dir(children), _) if !children.is_empty() => return Err(status::NOTEMPTY),
                        (Body::File(_) | Body::Link(_), proc::RMDIR) => return Err(status::NOTDIR),
                        _ => {}
                    }
                    let before = tree.wcc_before(target);
                    tree.unlink(target, &name);
                    tree.nodes.remove(&child);
                    let after = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| wcc(enc, before, &after)))
                }
                proc::RENAME => {
                    let from_name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let to_dir = tree.node_of(&args.opaque(64).map_err(|_| status::BADHANDLE)?)?;
                    let to_name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let moving = tree.child(target, &from_name).ok_or(status::NOENT)?;
                    let (from_before, to_before) = (tree.wcc_before(target), tree.wcc_before(to_dir));
                    if let Some(existing) = tree.child(to_dir, &to_name)
                        && existing != moving
                    {
                        let moving_is_dir = matches!(tree.nodes[&moving].body, Body::Dir(_));
                        match &tree.nodes[&existing].body {
                            Body::Dir(children) if !moving_is_dir => {
                                let _ = children;
                                return Err(status::ISDIR);
                            }
                            Body::Dir(children) if !children.is_empty() => return Err(status::NOTEMPTY),
                            Body::File(_) | Body::Link(_) if moving_is_dir => return Err(status::NOTDIR),
                            _ => {}
                        }
                        tree.unlink(to_dir, &to_name);
                        tree.nodes.remove(&existing);
                    }
                    if tree.child(to_dir, &to_name) != Some(moving) {
                        tree.unlink(target, &from_name);
                        let now = tree.tick();
                        tree.link(to_dir, &to_name, moving, now);
                    }
                    let (from_after, to_after) = (tree.attrs(target), tree.attrs(to_dir));
                    Ok(boxed(move |enc: &mut Encoder| {
                        wcc(enc, from_before, &from_after);
                        wcc(enc, to_before, &to_after);
                    }))
                }
                proc::READDIR | proc::READDIRPLUS => {
                    if procedure == proc::READDIRPLUS && knobs.no_readdirplus.load(Ordering::SeqCst) {
                        return Err(status::NOTSUPP);
                    }
                    let cookie = args.u64().unwrap_or(0);
                    let children = tree.children(target).ok_or(status::NOTDIR)?;
                    let page = knobs.page_entries.load(Ordering::SeqCst) as usize;
                    // Cookies 1 and 2 are `.` and `..`, which real servers list
                    // and the client drops by name.
                    let mut listed: Vec<(u64, Vec<u8>, u64)> =
                        vec![(1, b".".to_vec(), target), (2, b"..".to_vec(), target)];
                    listed.extend(
                        children
                            .iter()
                            .enumerate()
                            .map(|(index, (name, id))| (index as u64 + 3, name.clone(), *id)),
                    );
                    let total = listed.len() as u64;
                    let rows: Vec<(u64, Vec<u8>, u64, Fattr3, FileHandle)> = listed
                        .into_iter()
                        .filter(|(at, _, _)| *at > cookie)
                        .take(page)
                        .map(|(at, name, id)| (at, name, id, tree.attrs(id), tree.handle(id)))
                        .collect();
                    let eof = rows.last().is_none_or(|(at, ..)| *at == total);
                    let dir = tree.attrs(target);
                    let plus = procedure == proc::READDIRPLUS;
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&dir));
                        enc.fixed_opaque(&[0u8; 8]);
                        for (at, name, id, attrs, handle) in &rows {
                            enc.bool(true);
                            enc.u64(*id);
                            enc.opaque(name);
                            enc.u64(*at);
                            if plus {
                                nfs3_wire::post_op_attr(enc, Some(attrs));
                                nfs3_wire::post_op_fh(enc, Some(handle));
                            }
                        }
                        enc.bool(false);
                        enc.bool(eof);
                    }))
                }
                proc::FSSTAT => {
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        for value in [1_000_000_u64, 400_000, 300_000, 1000, 500, 500] {
                            enc.u64(value);
                        }
                        enc.u32(0);
                    }))
                }
                proc::FSINFO => {
                    let size = knobs.transfer_size.load(Ordering::SeqCst);
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        encode_fsinfo(enc, size);
                    }))
                }
                proc::PATHCONF => {
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.u32(32_000);
                        enc.u32(255);
                        enc.bool(true);
                        enc.bool(true);
                        enc.bool(false);
                        enc.bool(true);
                    }))
                }
                _ => Err(status::NOTSUPP),
            }
        })();
        match outcome {
            Ok(body) => success_reply(xid, |enc| {
                enc.u32(0);
                body(enc);
            }),
            Err(status) => success_reply(xid, |enc| enc.u32(status)),
        }
    }
}

/// A reply's results after its `NFS3_OK`.
type Results = Box<dyn FnOnce(&mut Encoder)>;

fn boxed(results: impl FnOnce(&mut Encoder) + 'static) -> Results {
    Box::new(results)
}

/// What the fake answers FSINFO with, `size` as both preferences.
fn encode_fsinfo(enc: &mut Encoder, size: u32) {
    enc.u32(1024 * 1024);
    enc.u32(size);
    enc.u32(4096);
    enc.u32(1024 * 1024);
    enc.u32(size);
    enc.u32(4096);
    enc.u32(size);
    enc.u64(u64::MAX);
    enc.u32(0);
    enc.u32(1);
    enc.u32(0x1b);
}

fn fake_fsinfo(size: u32) -> crate::proto::nfs3::FsInfo {
    crate::proto::nfs3::FsInfo {
        rtmax: 1024 * 1024,
        rtpref: size,
        wtmax: 1024 * 1024,
        wtpref: size,
        dtpref: size,
        maxfilesize: u64::MAX,
        time_delta: NfsTime {
            seconds: 0,
            nseconds: 1,
        },
        properties: 0x1b,
    }
}

fn wcc(enc: &mut Encoder, before: (u64, NfsTime, NfsTime), after: &Fattr3) {
    enc.bool(true);
    enc.u64(before.0);
    enc.u32(before.1.seconds);
    enc.u32(before.1.nseconds);
    enc.u32(before.2.seconds);
    enc.u32(before.2.nseconds);
    nfs3_wire::post_op_attr(enc, Some(after));
}

/// The parts of a `sattr3` the fake applies.
struct SetAttrs {
    mode: Option<u32>,
    size: Option<u64>,
    atime: Option<NfsTime>,
    mtime: Option<NfsTime>,
}

fn decode_sattr(args: &mut Decoder) -> SetAttrs {
    let flag_u32 = |args: &mut Decoder| {
        if args.bool().unwrap_or(false) {
            args.u32().ok()
        } else {
            None
        }
    };
    let mode = flag_u32(args);
    let _uid = flag_u32(args);
    let _gid = flag_u32(args);
    let size = if args.bool().unwrap_or(false) {
        args.u64().ok()
    } else {
        None
    };
    let time = |args: &mut Decoder| match args.u32().unwrap_or(0) {
        2 => Some(NfsTime {
            seconds: args.u32().unwrap_or(0),
            nseconds: args.u32().unwrap_or(0),
        }),
        _ => None,
    };
    let atime = time(args);
    let mtime = time(args);
    SetAttrs {
        mode,
        size,
        atime,
        mtime,
    }
}

/// Whether `slot` held `procedure` (+1), clearing it if so.
fn take_once(slot: &AtomicU32, procedure: u32) -> bool {
    slot.compare_exchange(procedure + 1, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

enum Outgoing {
    Reply { bytes: Bytes, is_read: bool },
    CutAndClose(Bytes),
}

/// Sends replies in order, except READs while reordering is on: those are
/// held until four are waiting (or 5 ms pass) and go out last first.
async fn write_replies<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut outgoing: mpsc::UnboundedReceiver<Outgoing>,
    knobs: Arc<Knobs>,
) {
    let mut held: Vec<Bytes> = Vec::new();
    loop {
        let next = if held.is_empty() {
            outgoing.recv().await
        } else {
            match tokio::time::timeout(Duration::from_millis(5), outgoing.recv()).await {
                Ok(next) => next,
                Err(_) => {
                    for reply in held.drain(..).rev() {
                        if writer.write_all(&reply).await.is_err() {
                            return;
                        }
                    }
                    continue;
                }
            }
        };
        match next {
            None => return,
            Some(Outgoing::Reply { bytes, is_read }) if is_read && knobs.reorder_reads.load(Ordering::SeqCst) => {
                held.push(bytes);
                if held.len() >= 4 {
                    for reply in held.drain(..).rev() {
                        if writer.write_all(&reply).await.is_err() {
                            return;
                        }
                    }
                }
            }
            Some(Outgoing::Reply { bytes, .. }) => {
                if writer.write_all(&bytes).await.is_err() {
                    return;
                }
            }
            Some(Outgoing::CutAndClose(bytes)) => {
                let _ = writer.write_all(&bytes[..bytes.len() / 2]).await;
                let _ = writer.shutdown().await;
                return;
            }
        }
    }
}
