//! What a PUT that didn't finish left at its key, and clearing it.
//!
//! ❗ Not every server keeps S3's promise to publish nothing short of
//! `Content-Length`: VersityGW stores whatever arrived before the connection
//! dropped, under the user's name (`apps/desktop/test/s3-servers/README.md`).
//! So every PUT carries a token of its own (`x-amz-meta-cmdr-write`), and a PUT
//! that was cancelled or cut off removes the object at its key ONLY when that
//! object carries its token and its HEAD shows it shorter than what the PUT
//! sent ([`S3Volume::settle_cut_off_put`]); no length, or a longer one, is left.
//! ❗ Ours at the full size means the server published before the link died:
//! that's the write landing, never a leftover.
//!
//! That cleanup needs the link that just died, so on a provider not trusted to
//! refuse a short body a PUT is also on record in the upload ledger until it's
//! known settled ([`PutRecord`]). What stays is settled by the next connect's
//! sweep and, first, by the next write to the key
//! ([`S3Volume::claim_for_write`]), so the engine's retry after an outage
//! lands instead of meeting our own truncated body as "already exists" (#522).

use std::time::Duration;

use log::warn;

use super::upload::{Progress, WriteTarget};
use super::upload_ledger::{KeyClaim, UnsettledPut, UploadLedger};
use super::{S3Volume, S3VolumeInner};
use crate::transport::{Answer, S3Client};

/// What a PUT that didn't finish left at its key, once settled.
#[derive(Debug)]
pub(super) enum CutOff {
    /// Ours, whole: the server published before the link died, so the write
    /// landed. The HEAD that showed it.
    Landed(Answer),
    /// Nothing of ours to clear: never stored, removed, the name holds someone
    /// else's object, or one the HEAD can't prove is our short leftover.
    Clear,
    /// Not known yet: the server couldn't be asked, the removal failed, or it
    /// had stored nothing so far. The PUT's record stays (`upload_ledger.rs`).
    Unsettled,
}

/// A PUT's record in the ledger, kept open from just before its first request
/// until the write is known settled. Dropped unsettled (an error, a future
/// dropped mid-request by the silence watch or the quit deadline), it leaves
/// the record for the next sweep or write to the key, which is what clears a
/// cut-off body the write's own cleanup couldn't reach (#522).
pub(super) struct PutRecord<'a> {
    ledger: &'a UploadLedger,
    put: UnsettledPut,
    started: bool,
    settled: bool,
}

impl PutRecord<'_> {
    /// Opens the record, once, right before a request goes out.
    pub(super) fn start(&mut self) {
        if !self.started {
            self.started = true;
            self.ledger.started(&self.put);
        }
    }

    /// Nothing of this PUT is left to clear.
    fn settle(&mut self) {
        if self.started && !self.settled {
            self.settled = true;
            self.ledger.finished(&self.put);
        }
    }
}

/// Settles `record`, when there is one.
pub(super) fn settle(record: &mut Option<PutRecord<'_>>) {
    if let Some(record) = record.as_mut() {
        record.settle();
    }
}

impl Drop for PutRecord<'_> {
    fn drop(&mut self) {
        if self.started && !self.settled {
            self.ledger.abandoned(&self.put);
        }
    }
}

/// Whether a HEAD shows this write's own object (`token`) at the full `size`:
/// what a write that lost its answer left when it landed whole.
fn is_ours_whole(head: &Answer, token: &str, size: u64) -> bool {
    head.header(crate::metadata::WRITE_TOKEN_HEADER) == Some(token) && head.object_length() == Some(size)
}

impl S3Volume {
    /// The ledger record for a PUT of `size` bytes to `target`, or `None` on a
    /// provider trusted to publish nothing short of `Content-Length`
    /// (`refuses_short_body`), where a cut-off PUT leaves nothing to clear.
    pub(super) fn put_record<'a>(
        &'a self,
        client: &S3Client,
        target: &WriteTarget<'_>,
        size: u64,
    ) -> Option<PutRecord<'a>> {
        if client.profile().refuses_short_body {
            return None;
        }
        Some(PutRecord {
            ledger: &self.inner.ledger,
            put: UnsettledPut {
                account: self.inner.account(),
                bucket: target.bucket.to_string(),
                key: target.key.to_string(),
                token: target.metadata.write_token.clone()?,
                size,
            },
            started: false,
            settled: false,
        })
    }

    /// After a PUT of `size` bytes that was cancelled or failed without an
    /// answer, settles what it left at the key, by THIS write's token
    /// ([`S3VolumeInner::clear_cut_off`] has the rules). ❗ Ours whole is the write
    /// landing (the link died after the server published), answered as
    /// [`CutOff::Landed`] for the caller to report as written.
    ///
    /// The server stores a cut-off body once it notices the dropped
    /// connection, which may be a moment after the drop, so a miss is checked
    /// again twice, 150 ms apart. On a server that keeps the contract this
    /// costs one HEAD that finds nothing, plus two after short waits, and only
    /// for a PUT that didn't finish.
    pub(super) async fn settle_cut_off_put(&self, client: &S3Client, target: &WriteTarget<'_>, size: u64) -> CutOff {
        let Some(token) = target.metadata.write_token.as_deref() else {
            return CutOff::Unsettled;
        };
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            let Ok(head) = self.head_object(client, target.bucket, target.key, target.remote).await else {
                return CutOff::Unsettled;
            };
            let Some(head) = head else {
                continue;
            };
            return self
                .inner
                .clear_cut_off(client, target.bucket, target.key, target.remote, head, token, size)
                .await;
        }
        // Nothing yet, but a server that keeps cut-off bodies may still store
        // this one: the record stays for the next sweep or write here.
        CutOff::Unsettled
    }

    /// Holds the key `bucket`/`key` for a write about to go there, after
    /// settling what an earlier PUT to it left on record: a cut-off body the
    /// server kept while the link was down, which would otherwise refuse this
    /// write's no-overwrite check as "already exists" (#522). Keep the claim
    /// until the write is done.
    ///
    /// `None` when nothing is on record for the key, the common case: a sweep
    /// acts only on a record, and only a write to this key makes one, so
    /// there's nothing to hold it against and no write waits on another.
    pub(super) async fn claim_for_write(&self, client: &S3Client, bucket: &str, key: &str) -> Option<KeyClaim> {
        let account = self.inner.account();
        if self.inner.ledger.unsettled_puts_at(&account, bucket, key).is_empty() {
            return None;
        }
        let claim = UploadLedger::claim_key(&account, bucket, key).await;
        // Asked again under the claim: a sweep may have settled some meanwhile.
        for put in self.inner.ledger.unsettled_puts_at(&account, bucket, key) {
            self.inner.settle_unsettled_put(client, &put).await;
        }
        Some(claim)
    }

    /// After a `CompleteMultipartUpload` that failed: our whole object at the
    /// key (this write's token, `size` bytes) means the server completed and
    /// only its answer was lost. One HEAD, ❌ never a delete.
    pub(super) async fn landed_whole(&self, client: &S3Client, target: &WriteTarget<'_>, size: u64) -> Option<Answer> {
        let token = target.metadata.write_token.as_deref()?;
        let head = self
            .head_object(client, target.bucket, target.key, target.remote)
            .await
            .ok()
            .flatten()?;
        is_ours_whole(&head, token, size).then_some(head)
    }

    /// A write whose answer never came but whose whole object is at the key
    /// (`head`, carrying our token): reported as written, the pane patched
    /// from that HEAD the way a verified landing is.
    pub(super) fn published_after_all(
        &self,
        target: &WriteTarget<'_>,
        head: &Answer,
        size: u64,
        progress: &Progress<'_>,
    ) -> u64 {
        warn!(
            target: "volume",
            "s3: the answer to a write of {} never came, but the server published it whole",
            target.remote
        );
        self.remember_written(target, head);
        let _ = progress.at(size);
        size
    }
}

/// The connection-scoped half, which a sweep runs without a place.
impl S3VolumeInner {
    /// What a PUT of `size` bytes carrying `token` left at the key, as `head`
    /// shows it, and the cleanup it needs:
    ///
    /// - ❗ **Ours, at `size` bytes: the write landed whole.** ❌ Never
    ///   removed: by then it's the only copy of the new bytes, and on an
    ///   overwrite the original is already gone.
    /// - **Ours, provably SHORTER than `size`**: a server keeping a truncated
    ///   body against S3's contract (VersityGW does), so it's removed.
    /// - ❌ **Anything else is left alone**: the original, another writer's, and
    ///   ours where the HEAD can't prove it short (#522/#545). A HEAD with no
    ///   length shows nothing, and a LONGER body is a later write by a client
    ///   that kept our `x-amz-meta-*` token on overwrite. Either may be a whole
    ///   user file, and a delete there is unrecoverable, so the record is let go
    ///   instead.
    #[allow(
        clippy::too_many_arguments,
        reason = "one object's address (bucket, key, and the path errors name), what's at it, and what the write sent"
    )]
    async fn clear_cut_off(
        &self,
        client: &S3Client,
        bucket: &str,
        key: &str,
        remote: &str,
        head: Answer,
        token: &str,
        size: u64,
    ) -> CutOff {
        if head.header(crate::metadata::WRITE_TOKEN_HEADER) != Some(token) {
            return CutOff::Clear;
        }
        if is_ours_whole(&head, token, size) {
            return CutOff::Landed(head);
        }
        if !head.object_length().is_some_and(|kept| kept < size) {
            warn!(target: "volume", "s3: left {remote} alone: it holds a cut-off upload's token, but its length doesn't show it short");
            return CutOff::Clear;
        }
        match self.delete_key(client, bucket, key, remote).await {
            Ok(()) => {
                warn!(target: "volume", "s3: the server kept a cut-off upload of {remote}; removed it");
                CutOff::Clear
            }
            Err(e) => {
                warn!(target: "volume", "s3: the server kept a cut-off upload of {remote} and removing it failed: {e}");
                CutOff::Unsettled
            }
        }
    }

    /// Settles a PUT an earlier write left on record (`upload_ledger.rs`):
    /// what it kept at the key is cleared by [`Self::clear_cut_off`]'s rules,
    /// and a key holding nothing has nothing left to clear. The record stays
    /// only when the server couldn't be asked or the removal failed.
    /// ❗ The caller holds the key's claim, so nothing publishes there between
    /// the HEAD and the delete.
    pub(super) async fn settle_unsettled_put(&self, client: &S3Client, put: &UnsettledPut) {
        let remote = format!("/{}/{}", put.bucket, put.key);
        let settled = match self.head_object(client, &put.bucket, &put.key, &remote).await {
            Ok(None) => true,
            Ok(Some(head)) => !matches!(
                self.clear_cut_off(client, &put.bucket, &put.key, &remote, head, &put.token, put.size)
                    .await,
                CutOff::Unsettled
            ),
            Err(_) => false,
        };
        if settled {
            self.ledger.finished(put);
        }
    }

    /// The sweep's half: settles each of `puts` whose key no write holds. ❌ A
    /// held key is skipped, never waited for: the write holding it settles
    /// the key's records itself before it sends a byte.
    pub(super) async fn settle_unsettled_puts(&self, client: &S3Client, puts: &[UnsettledPut]) {
        for put in puts {
            let Some(_claim) = UploadLedger::try_claim_key(&put.account, &put.bucket, &put.key) else {
                continue;
            };
            self.settle_unsettled_put(client, put).await;
        }
    }
}
