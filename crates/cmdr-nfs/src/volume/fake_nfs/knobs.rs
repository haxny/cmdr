//! The fake's knobs: what a real server does only sometimes, each safe to
//! flip while calls are in flight, and the counters a cell reads back.

use std::sync::atomic::Ordering;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{CreateSeen, FakeNfs};
use crate::proto::nfs3::NfsTime;

impl FakeNfs {
    /// Answers READDIRPLUS with `NFS3ERR_NOTSUPP` (unfs3's shape), so the
    /// client falls back to READDIR + LOOKUP.
    pub(in crate::volume) fn refuse_readdirplus(&self, refuse: bool) {
        self.knobs.no_readdirplus.store(refuse, Ordering::SeqCst);
    }

    /// Reads every call and answers none, on every connection, the `NULL`
    /// probe's fresh one included: a NAS asleep, a cable pulled.
    pub(in crate::volume) fn go_silent(&self, silent: bool) {
        self.knobs.silent.store(silent, Ordering::SeqCst);
    }

    /// New connections are closed at once (a stopped `nfsd`), through both
    /// the listener and the duplex dialer.
    pub(in crate::volume) fn refuse_dials(&self, refuse: bool) {
        self.knobs.refuse_dials.store(refuse, Ordering::SeqCst);
    }

    /// READ replies go out in batches, last first.
    pub(in crate::volume) fn reorder_reads(&self, reorder: bool) {
        self.knobs.reorder_reads.store(reorder, Ordering::SeqCst);
    }

    /// Refuses a client whose source port is past 1023: `MNT3ERR_ACCES` on
    /// `MNT`, `NFS3ERR_PERM` on every NFS call but `NULL` (knfsd's shape,
    /// measured: `apps/desktop/test/nfs-servers/README.md` § "Reaching
    /// /secure").
    pub(in crate::volume) fn require_reserved_port(&self, require: bool) {
        self.knobs.require_reserved_port.store(require, Ordering::SeqCst);
    }

    /// Every READ answers at most `bytes` (0: as asked).
    pub(in crate::volume) fn answer_reads_short(&self, bytes: u32) {
        self.knobs.short_reads.store(bytes, Ordering::SeqCst);
    }

    /// Every WRITE stores and answers at most `bytes` (0: all of it).
    pub(in crate::volume) fn answer_writes_short(&self, bytes: u32) {
        self.knobs.short_writes.store(bytes, Ordering::SeqCst);
    }

    /// How many entries a listing page holds.
    pub(in crate::volume) fn set_page_entries(&self, entries: u32) {
        self.knobs.page_entries.store(entries.max(1), Ordering::SeqCst);
    }

    /// The `rtpref` / `wtpref` FSINFO answers.
    pub(in crate::volume) fn set_transfer_size(&self, bytes: u32) {
        self.knobs.transfer_size.store(bytes, Ordering::SeqCst);
    }

    /// A server reboot as WRITE and COMMIT see it: a new verifier, so data not
    /// yet committed counts as lost.
    pub(in crate::volume) fn rotate_verifier(&self) {
        self.knobs.verifier.fetch_add(1, Ordering::SeqCst);
    }

    /// The next reply to NFS procedure `procedure` goes unsent (the call
    /// itself still happened).
    pub(in crate::volume) fn drop_one_reply(&self, procedure: u32) {
        self.knobs.drop_one.store(procedure + 1, Ordering::SeqCst);
    }

    /// The next reply to NFS procedure `procedure` is cut halfway, and that
    /// connection closes.
    pub(in crate::volume) fn reset_mid_reply(&self, procedure: u32) {
        self.knobs.reset_one.store(procedure + 1, Ordering::SeqCst);
    }

    /// The verifier rotates as the `nth` call (1-based) of `procedure`
    /// arrives, before it's answered: a reboot landing exactly there.
    pub(in crate::volume) fn rotate_verifier_on(&self, procedure: u32, nth: usize) {
        self.knobs.rotate_on.lock_ignore_poison().push((procedure, nth));
    }

    /// The next call to `procedure` answers `status` (an `nfsstat3`) and
    /// changes nothing.
    pub(in crate::volume) fn refuse_next(&self, procedure: u32, status: u32) {
        self.knobs.refuse_next.lock_ignore_poison().insert(procedure, status);
    }

    /// The `wtmax` FSINFO answers (0: 1 MiB).
    pub(in crate::volume) fn set_write_max(&self, bytes: u32) {
        self.knobs.write_max.store(bytes, Ordering::SeqCst);
    }

    /// The most data any one WRITE carried.
    pub(in crate::volume) fn largest_write(&self) -> u32 {
        self.knobs.largest_write.load(Ordering::SeqCst)
    }

    /// What the last CREATE asked for.
    pub(in crate::volume) fn last_create(&self) -> Option<CreateSeen> {
        *self.knobs.last_create.lock_ignore_poison()
    }

    /// Every NFS call's procedure number, in arrival order.
    pub(in crate::volume) fn call_log(&self) -> Vec<u32> {
        self.knobs.log.lock_ignore_poison().clone()
    }

    /// The mode and the `(atime, mtime)` of what's at `path`.
    pub(in crate::volume) fn mode_and_times(&self, path: &str) -> Option<(u32, NfsTime, NfsTime)> {
        let tree = self.tree.lock_ignore_poison();
        let node = tree.nodes.get(&tree.by_path(path)?)?;
        Some((node.mode, node.atime, node.mtime))
    }

    /// How many NFS calls of `procedure` arrived.
    pub(in crate::volume) fn calls(&self, procedure: u32) -> usize {
        self.knobs
            .calls
            .lock_ignore_poison()
            .get(&procedure)
            .copied()
            .unwrap_or(0)
    }

    /// Every NFS call that arrived, `NULL` included.
    pub(in crate::volume) fn total_calls(&self) -> usize {
        self.knobs.calls.lock_ignore_poison().values().sum()
    }
}
