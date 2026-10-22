//! The window's arithmetic against a reader that answers BACKWARDS: the later a
//! chunk starts, the fewer times it yields, so the sooner it lands.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::read_window::{ChunkFuture, ReadWindow};

/// Counts reads in flight, and the most there ever were at once.
#[derive(Clone, Default)]
struct Backwards {
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

fn read(reader: Backwards, offset: u64, len: usize) -> ChunkFuture<(u64, usize)> {
    Box::pin(async move {
        let now = reader.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        reader.peak.fetch_max(now, Ordering::SeqCst);
        for _ in 0..50u64.saturating_sub(offset / 100) {
            tokio::task::yield_now().await;
        }
        reader.in_flight.fetch_sub(1, Ordering::SeqCst);
        (offset, len)
    })
}

async fn drain(window: &mut ReadWindow<Backwards, (u64, usize)>) -> Vec<(u64, usize)> {
    let mut chunks = Vec::new();
    while let Some(chunk) = window.next_chunk().await {
        chunks.push(chunk);
    }
    chunks
}

#[tokio::test]
async fn hands_chunks_back_in_file_order_with_a_short_last_one() {
    let mut window = ReadWindow::new(Backwards::default(), read, 1_000, 4_500, 4, 1_000);
    let chunks = drain(&mut window).await;
    assert_eq!(
        chunks,
        vec![(1_000, 1_000), (2_000, 1_000), (3_000, 1_000), (4_000, 500)]
    );
}

#[tokio::test]
async fn keeps_exactly_depth_reads_outstanding() {
    let reader = Backwards::default();
    let mut window = ReadWindow::new(reader.clone(), read, 0, 4_000, 3, 100);
    assert_eq!(drain(&mut window).await.len(), 40);
    assert_eq!(reader.peak.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn an_empty_range_reads_nothing() {
    let reader = Backwards::default();
    let mut window = ReadWindow::new(reader.clone(), read, 500, 500, 4, 100);
    assert!(window.next_chunk().await.is_none());
    assert_eq!(reader.peak.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_zero_chunk_size_still_makes_progress() {
    let mut window = ReadWindow::new(Backwards::default(), read, 0, 3, 0, 0);
    assert_eq!(drain(&mut window).await, vec![(0, 1), (1, 1), (2, 1)]);
}
