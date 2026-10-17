//! One fresh-ZIP pipeline run: the producer streaming into one unknown-length
//! destination write, the remote feed beside them, and the join that names how
//! each participant ended.

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;

use super::fresh_plan::{FreshPlan, RemoteFeed};
use super::fresh_zip::{FreshZipCancellation, FreshZipError, FreshZipProgressObserver, spawn_fresh_zip_with_progress};
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, Volume, VolumeError, WriteMode};
use tokio::runtime::Handle;

/// Runs one producer into one unknown-length destination write and joins every
/// participant: the writer, the producer thread, and the remote feed task. The
/// feed and the producer's join run on `runtime` (the engine's).
pub(super) async fn produce_into(
    runtime: &Handle,
    plan: FreshPlan,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    level: Option<i64>,
    progress: FreshZipProgressObserver,
    cancellation: FreshZipCancellation,
) -> (Result<u64, VolumeError>, Result<u64, FreshZipError>) {
    let source_volume = Arc::clone(&plan.source_volume);
    let output = match spawn_fresh_zip_with_progress(plan.entries, level, Some(progress), cancellation.clone()) {
        Ok(output) => output,
        Err(error) => return (Err(VolumeError::NotSupported), Err(error)),
    };
    let (stream, completion) = output.into_parts();
    let feed_task = runtime.spawn(feed_remote(plan.remote_feeds, source_volume, cancellation.clone()));
    let callback = |_tick: StreamWriteProgress| continue_unless(&cancellation);
    let written = dest_volume
        .write_from_stream(
            &dest_path,
            WriteMode::CreateNew,
            StreamLength::Unknown,
            Box::new(stream),
            &callback,
        )
        .await
        .map(|written| written.bytes);
    let produced = completion.finish(runtime).await;
    if produced.is_err() {
        // A producer that ended early leaves the feed nothing to feed; stop a
        // source read that may never answer instead of awaiting it.
        cancellation.request();
    }
    let feed_result = feed_task
        .await
        .map_err(|_| FreshZipError::ProducerPanicked)
        .and_then(|result| result);
    let produced = match (produced, feed_result) {
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    (written, produced)
}

/// Breaks a destination write at its next acknowledgement once the pipeline's
/// one cancellation source fires; that callback is how SFTP, SMB, and ADB hear a
/// cancel mid-write.
pub(super) fn continue_unless(cancellation: &FreshZipCancellation) -> ControlFlow<()> {
    if cancellation.is_requested() {
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
}

/// Feeds each remote entry's bytes through its bridge, one entry at a time.
///
/// Every source read races the pipeline's cancellation: a `ChannelReadStream`
/// read is a queue wait, and dropping the stream is the backend's documented
/// stop. Opening a stream is one protocol round trip and is allowed to finish,
/// as the copy engine does. Returning drops every remaining feeder, which is
/// what wakes a producer parked on the bridge.
async fn feed_remote(
    feeds: Vec<RemoteFeed>,
    source_volume: Arc<dyn Volume>,
    cancellation: FreshZipCancellation,
) -> Result<(), FreshZipError> {
    for feed in feeds {
        if cancellation.is_requested() {
            return Err(FreshZipError::Cancelled);
        }
        let mut stream = match source_volume.open_read_stream(&feed.path).await {
            Ok(stream) => stream,
            Err(error) => {
                let source_error = FreshZipError::Source {
                    entry: feed.entry_name.clone(),
                    message: error.to_string(),
                };
                feed.feeder.send(Err(source_error.clone()), &cancellation).await?;
                return Err(source_error);
            }
        };
        loop {
            let chunk = tokio::select! {
                biased;
                () = cancellation.cancelled() => return Err(FreshZipError::Cancelled),
                chunk = stream.next_chunk() => chunk,
            };
            let Some(chunk) = chunk else { break };
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(error) => {
                    let source_error = FreshZipError::Source {
                        entry: feed.entry_name.clone(),
                        message: error.to_string(),
                    };
                    feed.feeder.send(Err(source_error.clone()), &cancellation).await?;
                    return Err(source_error);
                }
            };
            feed.feeder.send(Ok(bytes), &cancellation).await?;
        }
        feed.feeder.finish(&cancellation).await?;
    }
    Ok(())
}
