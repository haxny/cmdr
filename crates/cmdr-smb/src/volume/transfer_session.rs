//! The transfer session: ONE extra smb2 session (its own TCP connection) that a
//! share's byte-moving calls ride, so browsing that share stays responsive while
//! a copy runs on it.
//!
//! # Why
//!
//! One SMB connection is strictly ordered both ways. A listing response waits
//! behind every READ response queued ahead of it on the socket, a listing
//! request waits behind every WRITE frame already in the send queue, and the
//! server (ksmbd, Samba) serializes per connection on top of that. So while a
//! transfer shared the pane's session, navigating that share stalled for as long
//! as whatever was in flight took to drain: 23 s for a `stat` behind an 8 MiB
//! download at 375 KB/s, and 5–12 s behind WRITE frames on a loaded QNAP
//! (ERR-CNK7M). A second connection removes the SMB-level and the server-level
//! head-of-line blocking. ❗ It can't remove the LINK's: two connections over one
//! saturated Wi-Fi or WAN hop still share its queue. Numbers: `DETAILS.md`
//! § "The transfer session".
//!
//! # Shape
//!
//! - **What rides it**: the byte-moving calls only. Download streams (the hinted
//!   compound read included), uploads (the compound write included), and the
//!   server-side copy. Everything the pane does (listings, `stat`, create,
//!   rename, delete) stays on the main session, and so do the copy's own
//!   destination probes and the partial-file cleanup.
//! - **One per share, opened lazily** by the first transfer's lease and closed
//!   [`TRANSFER_SESSION_IDLE`] after the last lease drops, so a burst of small
//!   copies doesn't churn TCP and auth. Concurrent transfers multiplex over it
//!   exactly as they did over the main session. The first open is AWAITED: a
//!   lone big copy must not ride the main session for its whole length because
//!   its first file got there first.
//! - **A member is a full `SmbClient` + `Tree`** from the same `build_session`
//!   the main path uses (`Connection::clone` only multiplexes over ONE session).
//!
//! # Failure handling: the worst case is today's behavior
//!
//! - A transfer that loses this session fails with its typed error, the engine's
//!   per-file retry runs the file again (`transfer/retry.rs`), and that retry
//!   rides the main session while this one reconnects in the background (bounded
//!   backoff, single-flight). A loss here ❌ never moves the main session's state.
//! - An open the server refuses (a session cap, a transient failure) or a
//!   reconnect that runs out of backoff leaves the slot `Down` until the share
//!   goes idle: every transfer falls back to the main session meanwhile, which is
//!   exactly the pre-transfer-session behavior. An auth refusal gives up at once:
//!   the main session owns the credential dance.
//! - The share must read `Direct` for a lease at all: the main session is the
//!   one source of truth for "is the share up", which is what the transfer's
//!   outage wait (`transfer/volume_outage.rs`) asks.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;
use smb2::SmbClient;
use smb2::client::Connection;
use smb2::client::tree::Tree;
use smb2::types::SessionId;

use super::SmbConnectionParams;
use super::liveness::LiveConnection;
use super::session_build::{build_session, retired_session_id};
use super::state::TransferGeneration;

/// How long the transfer session stays open after its last lease drops.
///
/// Long enough to carry a person's next copy and the engine's gaps between
/// files (a conflict prompt, a folder's metadata pass) without a fresh TCP and
/// auth handshake; short enough that an idle share goes back to one connection
/// plus the watcher's soon after the work ends.
pub(super) const TRANSFER_SESSION_IDLE: Duration = Duration::from_secs(15);

/// Bounded, growing backoff for reconnecting a lost transfer session, the scan
/// pool's schedule (`scan_pool.rs`). Transfers ride the main session meanwhile,
/// so this is an accelerator coming back, never a wait anyone sits in.
const RECONNECT_BACKOFF: [Duration; 3] = [Duration::from_secs(1), Duration::from_secs(3), Duration::from_secs(10)];

/// The installed transfer session.
pub(super) struct Member {
    /// Owns the connection. Locked ONLY to follow a DFS link (`dfs_links.rs`),
    /// which needs `&mut SmbClient` for the referral and the open; a lease clones
    /// [`conn`](Self::conn) instead, so leases never wait on this.
    pub(super) client: Arc<tokio::sync::Mutex<SmbClient>>,
    /// A clone of the client's connection, taken at install, which every lease
    /// clones again. All clones multiplex over this member's one session.
    conn: Connection,
    pub(super) tree: Arc<Tree>,
    generation: TransferGeneration,
}

enum Slot {
    /// Nothing open and nothing wrong: the next lease opens one.
    Closed,
    Up(Arc<Member>),
    /// Lost, refused, or still reconnecting: leases fall back to the main
    /// session until a reconnect installs one or the share goes idle.
    Down,
}

/// A lease on the transfer session: the tree and a connection clone to drive a
/// transfer on, and the hold that keeps the session open while the transfer
/// uses them.
pub(super) struct Leased {
    pub(super) member: Arc<Member>,
    pub(super) conn: Connection,
    pub(super) generation: TransferGeneration,
    pub(super) hold: TransferHold,
}

/// Keeps the transfer session open while a transfer is using it. ❗ Lives as
/// long as the transfer's connection clone does (a download's producer task
/// carries it), or the idle close could drop the session under a live stream.
pub(super) struct TransferHold {
    session: Arc<TransferSession>,
}

impl Drop for TransferHold {
    fn drop(&mut self) {
        // The slot lock orders this against a lease taking a hold, so a close
        // can't land between a lease's clone and its count.
        let last = {
            let _slot = self.session.slot.lock_ignore_poison();
            self.session.active.fetch_sub(1, Ordering::AcqRel) == 1
        };
        if last {
            self.session.went_idle();
        }
    }
}

/// One share's transfer session. See the module docs.
pub(super) struct TransferSession {
    slot: Mutex<Slot>,
    /// Single-flight guard for the inline open, so concurrent first transfers
    /// wait for ONE login rather than each dialing their own. Held across
    /// `build_session`, which smb2 bounds.
    opening: tokio::sync::Mutex<()>,
    /// Leases outstanding. Changed only under `slot`'s lock.
    active: AtomicUsize,
    /// When `active` last reached zero, for the idle close.
    idle_since: Mutex<Option<Instant>>,
    idle_timer_running: AtomicBool,
    reconnecting: AtomicBool,
    /// Wakes a reconnect sleeping out its backoff when the main session comes back.
    share_back: tokio::sync::Notify,
    /// The lost member's session id, for the reconnect that replaces it to name
    /// (`session_build::retired_session_id`). ❌ Never the main session's.
    retired: Mutex<Option<SessionId>>,
    /// The installed member's connection, readable without any lock, so the
    /// transfer watchdog asks THIS connection whether the server is still
    /// talking (`liveness.rs`). Swapped in lockstep with the slot.
    pub(super) live: LiveConnection,
    /// Whether a member is installed right now. A hint for the readers that
    /// must not lock (`credit_copy_capacity`'s owner, the liveness choice).
    up: AtomicBool,
    next_generation: AtomicU64,
    /// Set by `on_unmount`: nothing opens or reconnects afterwards.
    closed: AtomicBool,
    /// [`TRANSFER_SESSION_IDLE`], in milliseconds; a suite shortens it.
    idle_ms: AtomicU64,
    /// The share's connection params, read fresh at every open, so a member
    /// built after a sign-in uses the credentials that worked.
    params: Arc<tokio::sync::RwLock<SmbConnectionParams>>,
    volume_id: String,
    /// Where reconnects and the idle timer spawn. ❌ Never `tokio::spawn`: a
    /// lease can drop on a thread with no ambient reactor.
    runtime: tokio::runtime::Handle,
}

impl TransferSession {
    pub(super) fn new(
        params: Arc<tokio::sync::RwLock<SmbConnectionParams>>,
        volume_id: String,
        runtime: tokio::runtime::Handle,
    ) -> Arc<Self> {
        Arc::new(Self {
            slot: Mutex::new(Slot::Closed),
            opening: tokio::sync::Mutex::new(()),
            active: AtomicUsize::new(0),
            idle_since: Mutex::new(None),
            idle_timer_running: AtomicBool::new(false),
            reconnecting: AtomicBool::new(false),
            share_back: tokio::sync::Notify::new(),
            retired: Mutex::new(None),
            live: LiveConnection::new(None),
            up: AtomicBool::new(false),
            next_generation: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            idle_ms: AtomicU64::new(TRANSFER_SESSION_IDLE.as_millis() as u64),
            params,
            volume_id,
            runtime,
        })
    }

    /// Whether a transfer session is installed right now.
    pub(super) fn is_up(&self) -> bool {
        self.up.load(Ordering::Acquire)
    }

    /// A lease on the transfer session, opening it first if nothing is open, or
    /// `None` when it's down (the caller falls back to the main session).
    pub(super) async fn lease(self: &Arc<Self>) -> Option<Leased> {
        loop {
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            if let Ok(leased) = self.try_lease() {
                return leased;
            }
            let _opening = self.opening.lock().await;
            if matches!(*self.slot.lock_ignore_poison(), Slot::Closed) {
                self.open().await;
            }
            // Loop: the slot is Up, Down, or (an unmount raced) closed now.
        }
    }

    /// The lease, without waiting: `Ok(None)` when down, `Err` when nothing is
    /// open yet.
    fn try_lease(self: &Arc<Self>) -> Result<Option<Leased>, NeedsOpen> {
        let slot = self.slot.lock_ignore_poison();
        match &*slot {
            Slot::Up(member) => {
                self.active.fetch_add(1, Ordering::AcqRel);
                *self.idle_since.lock_ignore_poison() = None;
                Ok(Some(Leased {
                    member: Arc::clone(member),
                    conn: member.conn.clone(),
                    generation: member.generation,
                    hold: TransferHold {
                        session: Arc::clone(self),
                    },
                }))
            }
            Slot::Down => Ok(None),
            Slot::Closed => Err(NeedsOpen),
        }
    }

    /// The first transfer's open, inline. A failure leaves the slot `Down` and
    /// kicks the background reconnect (auth aside), so a transient refusal
    /// self-heals while transfers ride the main session.
    async fn open(self: &Arc<Self>) {
        let params = self.params.read().await.clone();
        match build_session(&params, None).await {
            Ok((client, tree)) => {
                if self.install(client, tree) {
                    log::debug!("smb transfer session: opened for '{}'", self.volume_id);
                }
            }
            Err(e) => {
                let auth = crate::is_auth_error(&e);
                log::debug!(
                    "smb transfer session for volumeId={:?} didn't open: backend=smb2, error_kind={:?}, nt_status={:?}, detail={:?}; transfers ride the main session",
                    self.volume_id,
                    e.kind(),
                    e.status(),
                    cmdr_fs::log_detail::LogDetail(&e.to_string())
                );
                *self.slot.lock_ignore_poison() = Slot::Down;
                if !auth {
                    self.spawn_reconnect();
                }
            }
        }
    }

    /// Puts a freshly built session in the slot. `false` (and the session
    /// dropped) when the share was unmounted meanwhile.
    fn install(self: &Arc<Self>, mut client: SmbClient, tree: Tree) -> bool {
        let conn = client.connection_mut().clone();
        let generation = TransferGeneration::new(self.next_generation.fetch_add(1, Ordering::Relaxed));
        let mut slot = self.slot.lock_ignore_poison();
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        self.live.replace(Some(conn.clone()));
        *slot = Slot::Up(Arc::new(Member {
            client: Arc::new(tokio::sync::Mutex::new(client)),
            conn,
            tree: Arc::new(tree),
            generation,
        }));
        self.up.store(true, Ordering::Release);
        // A session installed into an idle share still closes on time.
        drop(slot);
        if self.active.load(Ordering::Acquire) == 0 {
            self.went_idle();
        }
        true
    }

    /// A call on session `generation` met an error that ends it: take it out of
    /// the slot and reconnect in the background. A loss from a session a
    /// reconnect already replaced moves nothing.
    pub(super) fn lose(self: &Arc<Self>, generation: TransferGeneration) {
        let lost = {
            let mut slot = self.slot.lock_ignore_poison();
            match &*slot {
                Slot::Up(member) if member.generation == generation => {
                    let Slot::Up(member) = std::mem::replace(&mut *slot, Slot::Down) else {
                        unreachable!("matched Up above");
                    };
                    self.up.store(false, Ordering::Release);
                    self.live.replace(None);
                    Some(member)
                }
                _ => None,
            }
        };
        let Some(member) = lost else { return };
        if let Ok(client) = member.client.try_lock()
            && let Some(id) = retired_session_id(&client)
        {
            *self.retired.lock_ignore_poison() = Some(id);
        }
        log::info!(
            "smb transfer session for '{}' lost; transfers ride the main session while it reconnects",
            self.volume_id
        );
        self.spawn_reconnect();
    }

    /// The main session just came back (`do_attempt_reconnect` installed one):
    /// a down transfer session tries again now rather than at its next backoff
    /// step, so the share's transfers leave the main session as soon as they
    /// can. A no-op while up, closed, or never opened.
    pub(super) fn share_is_back(self: &Arc<Self>) {
        if !matches!(*self.slot.lock_ignore_poison(), Slot::Down) {
            return;
        }
        if self.reconnecting.load(Ordering::Acquire) {
            self.share_back.notify_one();
        } else {
            self.spawn_reconnect();
        }
    }

    /// Single-flight background reconnect.
    fn spawn_reconnect(self: &Arc<Self>) {
        if self.closed.load(Ordering::Acquire)
            || self
                .reconnecting
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return;
        }
        let session = Arc::clone(self);
        self.runtime.spawn(async move {
            session.reconnect().await;
            session.reconnecting.store(false, Ordering::Release);
        });
    }

    async fn reconnect(self: &Arc<Self>) {
        let replaces = self.retired.lock_ignore_poison().take();
        for delay in RECONNECT_BACKOFF {
            // An idle close or an unmount reset the slot: nothing to come back to.
            if self.closed.load(Ordering::Acquire) || !matches!(*self.slot.lock_ignore_poison(), Slot::Down) {
                return;
            }
            let params = self.params.read().await.clone();
            match build_session(&params, replaces).await {
                Ok((client, tree)) => {
                    if matches!(*self.slot.lock_ignore_poison(), Slot::Down) && self.install(client, tree) {
                        log::debug!("smb transfer session: reconnected for '{}'", self.volume_id);
                    }
                    return;
                }
                Err(e) if crate::is_auth_error(&e) => {
                    log::debug!(
                        "smb transfer session for '{}' rejected auth ({e}); transfers stay on the main session",
                        self.volume_id
                    );
                    return;
                }
                Err(e) => log::debug!(
                    "smb transfer session for '{}' reconnect stopped ({e}); backing off",
                    self.volume_id
                ),
            }
            // The main session coming back cuts the wait short: the share
            // answers again, so this one will too.
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = self.share_back.notified() => {}
            }
        }
        log::debug!(
            "smb transfer session for '{}' still down after backoff; transfers stay on the main session until the share goes idle",
            self.volume_id
        );
    }

    /// The last lease dropped: close the session once it has been idle for
    /// [`TRANSFER_SESSION_IDLE`]. One timer at a time; a lease taken meanwhile
    /// clears `idle_since`, and the timer finds it busy.
    fn went_idle(self: &Arc<Self>) {
        *self.idle_since.lock_ignore_poison() = Some(Instant::now());
        if self
            .idle_timer_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let session = Arc::clone(self);
        self.runtime.spawn(async move {
            session.idle_timer().await;
        });
    }

    async fn idle_timer(self: Arc<Self>) {
        loop {
            let since = *self.idle_since.lock_ignore_poison();
            let Some(since) = since else {
                // Busy again (or closed): the next last-lease-drop restarts the
                // timer. A drop landing between the read above and this store
                // found the flag still set and left, so look once more.
                self.idle_timer_running.store(false, Ordering::Release);
                let idle_again = self.idle_since.lock_ignore_poison().is_some();
                if idle_again
                    && self
                        .idle_timer_running
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    continue;
                }
                return;
            };
            let left = Duration::from_millis(self.idle_ms.load(Ordering::Relaxed)).saturating_sub(since.elapsed());
            if !left.is_zero() {
                tokio::time::sleep(left).await;
                continue;
            }
            let closed = {
                let mut slot = self.slot.lock_ignore_poison();
                if self.active.load(Ordering::Acquire) != 0 {
                    None
                } else {
                    self.up.store(false, Ordering::Release);
                    self.live.replace(None);
                    *self.idle_since.lock_ignore_poison() = None;
                    Some(std::mem::replace(&mut *slot, Slot::Closed))
                }
            };
            if let Some(Slot::Up(_)) = closed {
                log::debug!("smb transfer session: closed for '{}' after going idle", self.volume_id);
            }
            // `closed` drops here, outside the lock: dropping the client closes
            // its TCP connection.
        }
    }

    /// Unmount teardown (sync: `on_unmount` has no runtime). Drops the member;
    /// a transfer still holding a connection clone fails on its next frame,
    /// as it would on the main session.
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let dropped = {
            let mut slot = self.slot.lock_ignore_poison();
            self.up.store(false, Ordering::Release);
            self.live.replace(None);
            std::mem::replace(&mut *slot, Slot::Closed)
        };
        drop(dropped);
    }

    /// The installed member, for the suites that assert on which session a
    /// transfer rode.
    #[cfg(test)]
    pub(super) fn member_for_test(&self) -> Option<Arc<Member>> {
        match &*self.slot.lock_ignore_poison() {
            Slot::Up(member) => Some(Arc::clone(member)),
            _ => None,
        }
    }

    /// Whether the slot is `Down` (lost, refused, or reconnecting).
    #[cfg(test)]
    pub(super) fn is_down_for_test(&self) -> bool {
        matches!(*self.slot.lock_ignore_poison(), Slot::Down)
    }

    #[cfg(test)]
    pub(super) fn active_for_test(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }

    /// Whether nothing is open and nothing is wrong (the idle close's result).
    #[cfg(test)]
    pub(super) fn is_closed_for_test(&self) -> bool {
        matches!(*self.slot.lock_ignore_poison(), Slot::Closed)
    }

    #[cfg(test)]
    pub(super) fn set_idle_for_test(&self, idle: Duration) {
        self.idle_ms.store(idle.as_millis() as u64, Ordering::Relaxed);
    }

    /// The installed member's generation, for a suite that reports a loss on it.
    #[cfg(test)]
    pub(super) fn generation_for_test(&self) -> Option<TransferGeneration> {
        self.member_for_test().map(|m| m.generation)
    }
}

/// [`TransferSession::try_lease`]'s "nothing is open yet".
struct NeedsOpen;

impl Member {
    /// The session id this member's client runs, for the suites that tell a
    /// transfer session from the main one.
    #[cfg(test)]
    pub(super) async fn session_id_for_test(&self) -> SessionId {
        self.client.lock().await.session_id()
    }
}

#[cfg(test)]
#[path = "transfer_session_test.rs"]
mod transfer_session_test;
