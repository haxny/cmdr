//! The multipart upload, and the sweep that aborts the ones Cmdr left behind.
//!
//! One part size per upload (`multipart.rs`, equal parts for R2), up to
//! the profile's `upload_concurrency()` parts in flight, and ❗ no buffering beyond them:
//! a part is read from the source only when a slot is free, so peak memory is
//! part size × concurrency (256 MiB at the 64 MiB floor). A part is buffered at
//! all because a failed one is sent again ([`retry_after`]), and a stream can't
//! be read twice.
//!
//! ❗ **The upload is recorded before its first part and forgotten only once
//! it's completed or aborted** (`upload_ledger.rs`). Cancel and every failure
//! abort it on the spot and confirm the abort in the background
//! ([`S3Volume::abort_and_confirm_later`]); an abort never confirmed, or a future
//! dropped mid-upload, leaves the record for the next connect's sweep. Parts of
//! an unfinished upload are invisible in every listing and billed forever.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{SelfHandle, VolumeError, VolumeReadStream};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use log::{debug, info, warn};
use tokio_util::sync::CancellationToken;

use super::errors::map_s3_error;
use super::query::body_error;
use super::upload::{
    PROGRESS_TICK, PartReader, Progress, ShortStream, WriteTarget, overwrite_for, retry_after, size_mismatch,
};
use super::upload_body::{BodyWatch, Halt, HaltSlot, PauseHold, buffered_body};
use super::upload_ledger::{UnfinishedUpload, UploadLedger, ledger_account};
use super::{S3Volume, S3VolumeInner};
use crate::error::{S3Error, S3ErrorCode};
use crate::multipart::{MAX_PARTS, PartPlan};
use crate::ops;
use crate::profile::{ConditionalOp, NoOverwrite};
use crate::transport::{COMPLETE_BUDGET, QUERY_BUDGET, S3Client, map_exchange_error};
use crate::xml::build::CompletedPart;
use crate::xml::{parse_complete_multipart, parse_initiate_multipart};

/// Marks an upload abandoned in the ledger if its task ends without settling
/// it, which covers a future dropped mid-upload (the silence watch, the quit
/// deadline): the next connect's sweep aborts it.
pub(super) struct UploadGuard {
    pub(super) ledger: UploadLedger,
    pub(super) upload: UnfinishedUpload,
    pub(super) settled: bool,
}

impl Drop for UploadGuard {
    fn drop(&mut self) {
        if !self.settled {
            self.ledger.abandoned(&self.upload);
        }
    }
}

/// One part's upload, owning everything it needs so it can run beside the
/// others.
struct PartJob {
    client: Arc<S3Client>,
    bucket: String,
    key: String,
    upload_id: String,
    number: u32,
    bytes: Bytes,
    /// This part's bytes on the wire, for progress. Reset on a retry.
    handed: Arc<AtomicU64>,
    stop: CancellationToken,
    /// The operation's pause: no attempt starts while paused, and one the
    /// pause outlasts is set aside and sent again.
    pause: PauseHold,
    /// Set when the server answers that the upload is gone (`NoSuchUpload`).
    gone: Arc<AtomicBool>,
    volume_id: String,
    remote: String,
}

/// An S3 refusal of a part or a completion, in the `Volume` vocabulary.
/// `NoSuchUpload` is the upload ending under us, ❌ never the destination
/// being "not found": Garage ends one when another write replaces the object
/// mid-upload (`apps/desktop/test/s3-servers/README.md`), so `gone` tells the
/// caller to look for that clash.
pub(super) fn upload_refusal(error: &S3Error, remote: &str, gone: &AtomicBool) -> VolumeError {
    if error.code == S3ErrorCode::NoSuchUpload {
        return upload_ended(remote, gone);
    }
    map_s3_error(error, remote)
}

/// The upload is gone on the server: flags `gone` for the caller and says so.
fn upload_ended(remote: &str, gone: &AtomicBool) -> VolumeError {
    gone.store(true, Ordering::Relaxed);
    VolumeError::IoError {
        message: ErrorDetail::path_then(remote, ": the server ended the upload before it completed"),
        raw_os_error: None,
    }
}

/// Whether the server still has upload `upload_id` (`ListParts`), `None` when
/// it couldn't say. Only an explicit `NoSuchUpload` is `Some(false)`.
async fn upload_exists(client: &S3Client, bucket: &str, key: &str, upload_id: &str) -> Option<bool> {
    let request = ops::list_parts(client.profile(), bucket, key, upload_id).ok()?;
    let answer = client.exchange(request, QUERY_BUDGET).await.ok()?;
    if answer.status.is_success() {
        return Some(true);
    }
    (S3Error::from_response(answer.status, &answer.text()).code == S3ErrorCode::NoSuchUpload).then_some(false)
}

impl PartJob {
    /// Sends the part, again after a throttle (`SlowDown`, 503, 429), a server
    /// fault, or a transport failure, up to [`retry_after`]'s limit, and again
    /// after a pause set it aside, which costs no attempt.
    async fn send(self) -> Result<(CompletedPart, u64), VolumeError> {
        let length = self.bytes.len() as u64;
        let mut attempt = 0;
        loop {
            // ❌ No request is open across a pause: the server would drop it.
            if self.pause.stopped().await {
                return Err(VolumeError::Cancelled(self.volume_id.clone().into()));
            }
            attempt += 1;
            self.handed.store(0, Ordering::Relaxed);
            let request = ops::upload_part(
                self.client.profile(),
                &self.bucket,
                &self.key,
                &self.upload_id,
                self.number,
                length,
            )
            .map_err(|_| VolumeError::NotFound(self.remote.clone()))?;
            let halted = HaltSlot::default();
            let body = buffered_body(
                self.bytes.clone(),
                BodyWatch {
                    handed: Arc::clone(&self.handed),
                    stop: self.stop.clone(),
                    liveness: Arc::clone(self.client.liveness()),
                    pause: self.pause.clone(),
                    halted: halted.clone(),
                    last_piece: None,
                },
            );
            let failure = match self.client.upload(request, body).await {
                Ok(answer) if answer.status.is_success() => {
                    let etag = answer.header("etag").ok_or_else(|| VolumeError::IoError {
                        message: ErrorDetail::path_then(
                            self.remote.as_str(),
                            format!(": part {} answered without an ETag", self.number),
                        ),
                        raw_os_error: None,
                    })?;
                    return Ok((
                        CompletedPart {
                            number: self.number,
                            etag: etag.to_string(),
                        },
                        length,
                    ));
                }
                Ok(answer) => {
                    let error = S3Error::from_response(answer.status, &answer.text());
                    if !error.is_retryable() {
                        return Err(upload_refusal(&error, &self.remote, &self.gone));
                    }
                    map_s3_error(&error, &self.remote)
                }
                Err(_) if self.stop.is_cancelled() => {
                    return Err(VolumeError::Cancelled(self.volume_id.clone().into()));
                }
                Err(e) => match halted.get() {
                    Some(Halt::SetAside) => {
                        debug!(
                            target: "volume",
                            "s3: set part {} of {} aside for a pause; it goes again on resume",
                            self.number,
                            LogPrivate::file_path(&self.remote),
                        );
                        attempt -= 1;
                        continue;
                    }
                    Some(Halt::Stopped) => return Err(VolumeError::Cancelled(self.volume_id.clone().into())),
                    None => {
                        // ❗ A server can refuse a part before reading its
                        // body and drop the connection, and the reset then
                        // eats its `NoSuchUpload` (Garage, when another write
                        // ended the upload). So a part cut off mid-flight asks
                        // whether the upload is still there before going again.
                        let exists = tokio::select! {
                            () = self.stop.cancelled() => return Err(VolumeError::Cancelled(self.volume_id.clone().into())),
                            exists = upload_exists(&self.client, &self.bucket, &self.key, &self.upload_id) => exists,
                        };
                        if exists == Some(false) {
                            return Err(upload_ended(&self.remote, &self.gone));
                        }
                        map_exchange_error(&e, &self.volume_id, &self.remote)
                    }
                },
            };
            let Some(wait) = retry_after(attempt) else {
                return Err(failure);
            };
            warn!(
                target: "volume",
                "s3: part {} of {} failed (attempt {attempt}): {}; again in {wait:?}",
                self.number,
                LogPrivate::file_path(&self.remote),
                LogError(&failure)
            );
            tokio::select! {
                () = self.stop.cancelled() => return Err(VolumeError::Cancelled(self.volume_id.clone().into())),
                () = tokio::time::sleep(wait) => {}
            }
        }
    }
}

/// The source as parts: a known plan's exact sizes, or (for a stream of
/// unknown length) parts of the floor size until it ends.
enum PartSizes {
    Planned(PartPlan),
    Open { part_size: u64 },
}

impl PartSizes {
    /// The size of 1-based part `number`, `None` past the plan's last.
    fn size_of(&self, number: u32) -> Option<usize> {
        let size = match self {
            Self::Planned(plan) if number > plan.part_count => return None,
            Self::Planned(plan) => {
                let (first, last) = plan.range(number);
                last - first + 1
            }
            Self::Open { .. } if u64::from(number) > MAX_PARTS => return None,
            Self::Open { part_size } => *part_size,
        };
        usize::try_from(size).ok()
    }
}

impl S3Volume {
    /// A multipart upload of `stream` to `target`: `plan` for a known length,
    /// `None` for a stream whose length shows only at its end, and `short`
    /// for what such a stream does when it ends inside its first part.
    /// Returns the bytes written.
    pub(super) async fn upload_in_parts(
        &self,
        client: &Arc<S3Client>,
        target: &WriteTarget<'_>,
        plan: Option<PartPlan>,
        short: ShortStream,
        stream: Box<dyn VolumeReadStream>,
        progress: &Progress<'_>,
    ) -> Result<u64, VolumeError> {
        let sizes = match plan {
            Some(plan) => PartSizes::Planned(plan),
            None => PartSizes::Open {
                part_size: self.part_floor(),
            },
        };
        let mut reader = PartReader::new(stream);
        // A stream of unknown length that ends inside its first part is one
        // PUT after all, unless it's an overwrite that must not be one.
        let mut first = None;
        if let PartSizes::Open { part_size } = sizes {
            let volume_id = self.volume_id().to_string();
            let mut between = || match progress.at(0) {
                std::ops::ControlFlow::Break(()) => Err(VolumeError::Cancelled(volume_id.clone().into())),
                std::ops::ControlFlow::Continue(()) => Ok(()),
            };
            let head = reader
                .fill_with(usize::try_from(part_size).unwrap_or(usize::MAX), &mut between)
                .await?;
            if reader.at_end_with(&mut between).await? && (short == ShortStream::OnePut || head.is_empty()) {
                return self.put_whole(client, target, head, progress).await;
            }
            first = Some(head);
        }
        let check_first = target.mode.refuses_occupied()
            && client.profile().no_overwrite(ConditionalOp::CompleteMultipart) == NoOverwrite::CheckThenWrite;
        if check_first {
            self.refuse_if_taken(client, target).await?;
        }
        let (upload_id, mut guard) = self.start_recorded_upload(client, target).await?;
        let gone = Arc::new(AtomicBool::new(false));
        let sent = self
            .send_parts(client, target, &upload_id, &sizes, &mut reader, first, progress, &gone)
            .await;
        // ❗ The completion is what publishes, so a pause waits here, and a
        // Cancel that came in while the last part was in flight (or while
        // paused) is honoured here, before it.
        let sent = match sent {
            Ok(_) if progress.pause.stopped().await => Err(VolumeError::Cancelled(self.volume_id().to_string().into())),
            sent => sent,
        };
        let sent = sent.and_then(|(parts, total)| match progress.at(total) {
            std::ops::ControlFlow::Break(()) => Err(VolumeError::Cancelled(self.volume_id().to_string().into())),
            std::ops::ControlFlow::Continue(()) => Ok((parts, total)),
        });
        let outcome = match sent {
            Ok((parts, total)) => match self.complete(client, target, &upload_id, &parts, &gone).await {
                Ok(etag) => Ok((total, etag)),
                // ❗ The link can die after the server completed: then our
                // whole object is at the key, the upload no longer exists, and
                // the write landed. ❌ Never reported as a failure.
                Err(e) => match self.landed_whole(client, target, total).await {
                    Some(head) => {
                        self.inner.ledger.finished(&guard.upload);
                        guard.settled = true;
                        return Ok(self.published_after_all(target, &head, total, progress));
                    }
                    None => Err(e),
                },
            },
            Err(e) => Err(e),
        };
        match outcome {
            Ok((total, etag)) => {
                self.inner.ledger.finished(&guard.upload);
                guard.settled = true;
                let _ = progress.at(total);
                self.verify_landing(client, target, total, etag.as_deref()).await?;
                Ok(total)
            }
            Err(e) => {
                // One abort now; the rounds confirming it run afterwards, and
                // what they can't confirm stays for the next sweep.
                self.abort_and_confirm_later(client, guard).await;
                // An upload the server ended under a `CreateNew` was most
                // likely another writer taking the name: say so when the name
                // is taken now, which is the refusal a `CreateNew` owes.
                if gone.load(Ordering::Relaxed)
                    && target.mode.refuses_occupied()
                    && matches!(
                        self.head_object(client, target.bucket, target.key, target.remote).await,
                        Ok(Some(_))
                    )
                {
                    return Err(VolumeError::AlreadyExists(target.remote.to_string()));
                }
                Err(e)
            }
        }
    }

    /// `CreateMultipartUpload`, carrying the object's metadata.
    pub(super) async fn create_upload(
        &self,
        client: &S3Client,
        target: &WriteTarget<'_>,
    ) -> Result<String, VolumeError> {
        let request = ops::create_multipart_upload(client.profile(), target.bucket, target.key, target.metadata)
            .map_err(|_| VolumeError::NotFound(target.remote.to_string()))?;
        let answer = self.ask(client, request, target.remote).await?;
        let initiated = parse_initiate_multipart(&answer.text()).map_err(|e| body_error(&e, target.remote))?;
        debug!(target: "volume", "s3 multipart upload started for {}", LogPrivate::file_path(&target.remote));
        Ok(initiated.upload_id)
    }

    /// Starts a multipart upload and records it in the ledger before its first
    /// part, answering its id and the guard that aborts it unless settled.
    pub(super) async fn start_recorded_upload(
        &self,
        client: &S3Client,
        target: &WriteTarget<'_>,
    ) -> Result<(String, UploadGuard), VolumeError> {
        let upload_id = self.create_upload(client, target).await?;
        let upload = UnfinishedUpload {
            account: self.inner.account(),
            bucket: target.bucket.to_string(),
            key: target.key.to_string(),
            upload_id: upload_id.clone(),
        };
        self.inner.ledger.started(&upload);
        let guard = UploadGuard {
            ledger: self.inner.ledger.clone(),
            upload,
            settled: false,
        };
        Ok((upload_id, guard))
    }

    /// Reads parts from `reader` as slots free up and sends them, reporting
    /// progress every tick. Answers the completed parts in order and the bytes
    /// sent. ❗ Leaves the upload for the caller to complete or abort.
    #[allow(
        clippy::too_many_arguments,
        reason = "one upload's whole context: the client, the target, the upload id, the part sizes, the source, a first part already read, progress, and the upload-gone flag"
    )]
    async fn send_parts(
        &self,
        client: &Arc<S3Client>,
        target: &WriteTarget<'_>,
        upload_id: &str,
        sizes: &PartSizes,
        reader: &mut PartReader,
        first: Option<Vec<u8>>,
        progress: &Progress<'_>,
        gone: &Arc<AtomicBool>,
    ) -> Result<(Vec<CompletedPart>, u64), VolumeError> {
        let stop = CancellationToken::new();
        // Dropping the in-flight parts on any early return stops their
        // requests; the caller's abort then removes what landed.
        let _stop_on_exit = stop.clone().drop_guard();
        let mut in_flight = FuturesUnordered::new();
        let mut on_wire: HashMap<u32, Arc<AtomicU64>> = HashMap::new();
        let mut parts: Vec<CompletedPart> = Vec::new();
        let mut done_bytes = 0u64;
        let mut next = 1u32;
        let mut source_done = false;
        let mut pending_first = first;
        let mut tick = tokio::time::interval(PROGRESS_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            // ❗ A part is read only when a slot is free, so no more than
            // `upload_concurrency()` parts are ever in memory.
            while !source_done && in_flight.len() < client.profile().upload_concurrency() {
                let Some(want) = sizes.size_of(next) else {
                    source_done = true;
                    break;
                };
                let bytes = match pending_first.take() {
                    Some(first) => first,
                    None => {
                        // Progress and cancel keep answering while a slow
                        // source fills the part; the parts already sent go
                        // on in the background meanwhile.
                        let mut between = || {
                            let moving: u64 = on_wire.values().map(|handed| handed.load(Ordering::Relaxed)).sum();
                            match progress.at(done_bytes + moving) {
                                std::ops::ControlFlow::Break(()) => {
                                    stop.cancel();
                                    Err(VolumeError::Cancelled(self.volume_id().to_string().into()))
                                }
                                std::ops::ControlFlow::Continue(()) => Ok(()),
                            }
                        };
                        reader.fill_with(want, &mut between).await?
                    }
                };
                if bytes.is_empty() {
                    source_done = true;
                    break;
                }
                if let PartSizes::Planned(_) = sizes
                    && bytes.len() < want
                {
                    return Err(size_mismatch(target.remote, reader.read_so_far(), planned_total(sizes)));
                }
                let short = bytes.len() < want;
                let handed = Arc::new(AtomicU64::new(0));
                on_wire.insert(next, Arc::clone(&handed));
                in_flight.push(
                    PartJob {
                        client: Arc::clone(client),
                        bucket: target.bucket.to_string(),
                        key: target.key.to_string(),
                        upload_id: upload_id.to_string(),
                        number: next,
                        bytes: Bytes::from(bytes),
                        handed,
                        stop: stop.clone(),
                        pause: progress.pause.clone(),
                        gone: Arc::clone(gone),
                        volume_id: self.volume_id().to_string(),
                        remote: target.remote.to_string(),
                    }
                    .send(),
                );
                next += 1;
                // A short part of an open stream is its last.
                source_done |= short;
            }
            if in_flight.is_empty() {
                break;
            }
            tokio::select! {
                Some(finished) = in_flight.next() => {
                    let (part, length) = finished?;
                    on_wire.remove(&part.number);
                    done_bytes += length;
                    parts.push(part);
                }
                _ = tick.tick() => {
                    let moving: u64 = on_wire.values().map(|handed| handed.load(Ordering::Relaxed)).sum();
                    if progress.at(done_bytes + moving).is_break() {
                        stop.cancel();
                        return Err(VolumeError::Cancelled(self.volume_id().to_string().into()));
                    }
                }
            }
        }
        // ❗ A known length is a promise: bytes past the last part mean the
        // source changed under us, and the upload must not complete short.
        if let PartSizes::Planned(plan) = sizes {
            // Every part is in, so progress stands still while this waits.
            let mut between = || match progress.at(done_bytes) {
                std::ops::ControlFlow::Break(()) => Err(VolumeError::Cancelled(self.volume_id().to_string().into())),
                std::ops::ControlFlow::Continue(()) => Ok(()),
            };
            let more = !reader.at_end_with(&mut between).await?;
            if more || reader.taken != plan.total {
                return Err(size_mismatch(target.remote, reader.read_so_far(), plan.total));
            }
        }
        parts.sort_by_key(|part| part.number);
        Ok((parts, done_bytes))
    }

    /// `CompleteMultipartUpload`, refusing an occupied key the way the provider
    /// can: its header where allowlisted, else a check right before (which
    /// also catches a writer that took the name during the upload). Answers
    /// the object's ETag.
    ///
    /// ❗ The body is parsed even on 200: S3 can fail a completion inside it.
    pub(super) async fn complete(
        &self,
        client: &S3Client,
        target: &WriteTarget<'_>,
        upload_id: &str,
        parts: &[CompletedPart],
        gone: &AtomicBool,
    ) -> Result<Option<String>, VolumeError> {
        let mut retried_without_header = false;
        loop {
            let built = ops::complete_multipart_upload(
                client.profile(),
                target.bucket,
                target.key,
                upload_id,
                parts,
                overwrite_for(target.mode),
            )
            .map_err(|_| VolumeError::NotFound(target.remote.to_string()))?;
            let conditional = target.mode.refuses_occupied() && !built.check_first;
            // Checked again right before the object appears: a writer that took
            // the name during a long upload is caught here, not after.
            if built.check_first {
                self.refuse_if_taken(client, target).await?;
            }
            let answer = client
                .exchange(built.request, COMPLETE_BUDGET)
                .await
                .map_err(|e| map_exchange_error(&e, self.volume_id(), target.remote))?;
            if answer.status.is_success() {
                return parse_complete_multipart(&answer.text())
                    .map(|completed| completed.etag)
                    .map_err(|e| body_error(&e, target.remote));
            }
            let error = S3Error::from_response(answer.status, &answer.text());
            if conditional && error.is_not_implemented() && !retried_without_header {
                // The parts are still there, so unlike a PUT this one can be
                // sent again, checked the other way.
                client.profile().downgrade(ConditionalOp::CompleteMultipart);
                retried_without_header = true;
                continue;
            }
            return Err(upload_refusal(&error, target.remote, gone));
        }
    }
}

/// The total a known plan promised.
fn planned_total(sizes: &PartSizes) -> u64 {
    match sizes {
        PartSizes::Planned(plan) => plan.total,
        PartSizes::Open { .. } => 0,
    }
}

/// How many times an abort is sent before the upload is left for the next
/// sweep, and how long to wait between rounds.
const ABORT_ROUNDS: u32 = 4;
const ABORT_ROUND_GAP: Duration = Duration::from_millis(200);

/// Aborts `upload` and settles its record: forgotten once the server's own
/// listing no longer shows it, kept for the next sweep otherwise. Answers
/// whether it settled. The sweep's way, which runs in the background already;
/// a write's failure takes [`S3Volume::abort_and_confirm_later`].
///
/// ❗ An abort's 204 isn't proof. A part request cut off a moment earlier can
/// still land after it, and a server may then bring the upload back: AWS says
/// to abort again until the parts are gone, and VersityGW does resurrect it
/// (fixture README). So each round aborts and then lists the key's uploads,
/// and only a listing without this upload ID ends it.
pub(super) async fn abort_upload(client: &S3Client, ledger: &UploadLedger, upload: &UnfinishedUpload) -> bool {
    let settled = settle_abort(&OnClient { client, upload }, None).await;
    if settled {
        ledger.finished(upload);
    } else {
        warn!(target: "volume", "s3: couldn't abort the upload of {}; the next connect tries again", LogPrivate::file_path(&upload.key));
        ledger.abandoned(upload);
    }
    settled
}

/// What the abort rounds ask a server about one upload. Each answer is `None`
/// when there's no one left to ask (the volume was retired, or its session is
/// down), which ends the rounds and leaves the record for the next sweep.
pub(super) trait UploadEnd {
    /// Sends one abort: `Some(true)` when the server took it or no longer
    /// knows the upload.
    async fn abort(&self) -> Option<bool>;

    /// Whether the server still lists the upload; `Some(None)` when it
    /// answered nothing usable.
    async fn still_listed(&self) -> Option<Option<bool>>;
}

/// The rounds: abort, then list, until a listing without the upload ends it
/// (`true`), the rounds run out, or no one is left to ask (`false`).
/// `first_abort` is an abort already sent, whose answer round one reads
/// instead of sending another.
pub(super) async fn settle_abort(upload: &impl UploadEnd, mut first_abort: Option<bool>) -> bool {
    for round in 0..ABORT_ROUNDS {
        if round > 0 {
            tokio::time::sleep(ABORT_ROUND_GAP).await;
        }
        let answered = match first_abort.take() {
            Some(answered) => answered,
            None => match upload.abort().await {
                Some(answered) => answered,
                None => return false,
            },
        };
        if answered {
            match upload.still_listed().await {
                Some(Some(false)) => return true,
                Some(_) => {}
                None => return false,
            }
        }
    }
    false
}

/// One upload asked through a client in hand (the sweep's).
struct OnClient<'a> {
    client: &'a S3Client,
    upload: &'a UnfinishedUpload,
}

impl UploadEnd for OnClient<'_> {
    async fn abort(&self) -> Option<bool> {
        Some(send_abort(self.client, self.upload).await)
    }

    async fn still_listed(&self) -> Option<Option<bool>> {
        Some(still_listed(self.client, self.upload).await)
    }
}

/// One upload asked through whatever client the volume holds NOW, for rounds
/// that outlive the write that started them: a volume retired (evicted,
/// unmounted, replaced) or a session that's down ends them.
struct OnLiveVolume {
    volume: SelfHandle<S3VolumeInner>,
    upload: UnfinishedUpload,
}

impl OnLiveVolume {
    async fn client(&self) -> Option<Arc<S3Client>> {
        self.volume.live()?.session.installed_client().await
    }
}

impl UploadEnd for OnLiveVolume {
    async fn abort(&self) -> Option<bool> {
        Some(send_abort(&*self.client().await?, &self.upload).await)
    }

    async fn still_listed(&self) -> Option<Option<bool>> {
        Some(still_listed(&*self.client().await?, &self.upload).await)
    }
}

/// One `AbortMultipartUpload`: whether the server took it or no longer knows
/// the upload.
async fn send_abort(client: &S3Client, upload: &UnfinishedUpload) -> bool {
    let Ok(request) = ops::abort_multipart_upload(client.profile(), &upload.bucket, &upload.key, &upload.upload_id)
    else {
        // A key that can't be addressed can't have an upload under it.
        return true;
    };
    match client.exchange(request, QUERY_BUDGET).await {
        Ok(answer) => {
            answer.status.is_success() || S3Error::from_response(answer.status, &answer.text()).is_not_found()
        }
        Err(e) => {
            debug!(target: "volume", "s3: aborting an upload of {} didn't reach the server: {e}", LogPrivate::file_path(&upload.key));
            false
        }
    }
}

/// Whether the server still lists `upload` among its key's unfinished uploads,
/// `None` when it couldn't say.
async fn still_listed(client: &S3Client, upload: &UnfinishedUpload) -> Option<bool> {
    let request = ops::list_multipart_uploads(client.profile(), &upload.bucket, &upload.key, None).ok()?;
    let answer = client.exchange(request, QUERY_BUDGET).await.ok()?;
    if !answer.status.is_success() {
        return None;
    }
    let page = crate::xml::parse_list_multipart_uploads(&answer.text()).ok()?;
    // One page answers it: the listing is in key order and this key's uploads
    // come first under a prefix that IS the key.
    Some(page.uploads.iter().any(|listed| listed.upload_id == upload.upload_id))
}

impl S3Volume {
    /// A failed or cancelled write's abort: ONE abort sent now, so the write
    /// answers at once, and the confirming rounds (list, abort again while a
    /// late part brings the upload back) on the volume's runtime afterwards.
    ///
    /// The rounds hold `guard`, so the upload counts as in flight (no sweep
    /// races them) until they settle it; any other end (rounds run out, the
    /// volume retired or disconnected, the runtime shut down at quit and the
    /// task dropped) leaves the record for the next connect's sweep through
    /// the guard's `Drop`. Nothing waits on them.
    pub(super) async fn abort_and_confirm_later(&self, client: &S3Client, guard: UploadGuard) {
        let first = send_abort(client, &guard.upload).await;
        let upload = OnLiveVolume {
            volume: self.inner.session.self_handle(),
            upload: guard.upload.clone(),
        };
        self.inner.host.runtime().spawn(async move {
            let mut guard = guard;
            if settle_abort(&upload, Some(first)).await {
                guard.ledger.finished(&guard.upload);
                guard.settled = true;
            } else {
                warn!(target: "volume", "s3: couldn't abort the upload of {}; the next connect tries again", LogPrivate::file_path(&guard.upload.key));
            }
        });
    }
}

impl S3VolumeInner {
    /// The account a ledger record belongs to ([`ledger_account`]).
    pub(super) fn account(&self) -> String {
        ledger_account(&self.params())
    }

    /// Starts the sweep in the background, so a connect never waits on it.
    pub(super) fn spawn_upload_sweep(&self) {
        let handle = self.session.self_handle();
        self.host.runtime().spawn(async move {
            if let Some(inner) = handle.live() {
                inner.sweep_unfinished_uploads().await;
            }
        });
    }

    /// Aborts every upload this account recorded and never finished, ❌ and
    /// nothing else: an upload ID the ledger doesn't hold is another tool's,
    /// and may be live. Then settles every PUT left on record, which clears a
    /// cut-off body a server kept under the user's name while the link was
    /// down (#522). Answers how many uploads it aborted.
    pub(super) async fn sweep_unfinished_uploads(&self) -> usize {
        let ledger = self.ledger.clone();
        let account = self.account();
        let (leftovers, puts) =
            tokio::task::spawn_blocking(move || (ledger.leftovers(&account), ledger.unsettled_puts(&account)))
                .await
                .unwrap_or_default();
        if leftovers.is_empty() && puts.is_empty() {
            return 0;
        }
        let Some(client) = self.session.installed_client().await else {
            return 0;
        };
        self.settle_unsettled_puts(&client, &puts).await;
        if leftovers.is_empty() {
            return 0;
        }
        let mut aborted = 0;
        for upload in &leftovers {
            if abort_upload(&client, &self.ledger, upload).await {
                aborted += 1;
            }
        }
        info!(
            target: "volume",
            "s3: aborted {aborted} of {} unfinished uploads Cmdr left behind",
            leftovers.len()
        );
        aborted
    }
}
