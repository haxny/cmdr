//! The windows a chunked upload reads its source through: every byte once, in
//! order, whatever the source's pieces line up with.

use std::pin::Pin;

use cmdr_fs::volume::{StreamLength, VolumeError, VolumeReadStream};

use super::SharedSource;

/// A source yielding `bytes` in pieces of the given lengths, in turn.
struct Pieces {
    bytes: Vec<u8>,
    lengths: Vec<usize>,
    at: usize,
    next: usize,
}

fn pieces(total: usize, lengths: &[usize]) -> Box<dyn VolumeReadStream> {
    Box::new(Pieces {
        bytes: (0..total).map(|i| (i % 251) as u8).collect(),
        lengths: lengths.to_vec(),
        at: 0,
        next: 0,
    })
}

impl VolumeReadStream for Pieces {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.at >= self.bytes.len() {
                return None;
            }
            let len = self.lengths[self.next % self.lengths.len()];
            self.next += 1;
            let end = (self.at + len).min(self.bytes.len());
            let piece = self.bytes[self.at..end].to_vec();
            self.at = end;
            Some(Ok(piece))
        })
    }
    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.bytes.len() as u64)
    }
    fn bytes_read(&self) -> u64 {
        self.at as u64
    }
    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

/// Everything one window yields, read to its end.
async fn drain(source: &SharedSource, len: u64) -> Vec<u8> {
    let mut window = source.window(len);
    let mut out = Vec::new();
    while let Some(piece) = window.next_chunk().await {
        out.extend(piece.expect("the test source never fails"));
    }
    out
}

#[tokio::test]
async fn windows_hand_out_every_byte_once_across_ragged_pieces() {
    let source = SharedSource::new(pieces(23, &[5, 7, 3]));
    let mut joined = Vec::new();
    for len in [4, 4, 10, 5] {
        let window = drain(&source, len).await;
        assert_eq!(window.len() as u64, len);
        joined.extend(window);
    }
    let expected: Vec<u8> = (0..23).map(|i| (i % 251) as u8).collect();
    assert_eq!(joined, expected);
    assert!(source.ensure_drained("/a", 23).await.is_ok());
    assert_eq!(source.ran_dry().await, None);
}

#[tokio::test]
async fn a_full_window_answers_its_end_without_reading_on() {
    let source = SharedSource::new(pieces(10, &[10]));
    let mut window = source.window(4);
    assert_eq!(window.next_chunk().await.unwrap().unwrap().len(), 4);
    assert!(window.next_chunk().await.is_none());
    // The six bytes the first piece carried past the window wait for the next.
    assert_eq!(drain(&source, 6).await.len(), 6);
}

#[tokio::test]
async fn a_source_with_bytes_past_the_promise_is_refused() {
    let source = SharedSource::new(pieces(16, &[8]));
    drain(&source, 15).await;
    let refused = source.ensure_drained("/a.bin", 15).await;
    assert!(matches!(refused, Err(VolumeError::IoError { .. })), "{refused:?}");
}

#[tokio::test]
async fn a_source_that_ends_early_says_how_much_it_had() {
    let source = SharedSource::new(pieces(10, &[3]));
    assert_eq!(drain(&source, 12).await.len(), 10);
    assert_eq!(source.ran_dry().await, Some(10));
}
