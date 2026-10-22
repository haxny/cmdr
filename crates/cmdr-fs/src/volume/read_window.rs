//! The read window: `depth` positioned chunk reads in flight over one byte
//! range, handed back in FILE order however they complete. What makes a file
//! over a link with any latency in it arrive at more than one chunk per round
//! trip (`crates/cmdr-sftp/DETAILS.md` § "The read window" has the curve).
//!
//! The window owns only the arithmetic: which offset goes out next, how long it
//! is, and how many are outstanding. Filling one chunk (short answers, where the
//! file ends) is the backend's own `read` function, since every protocol says
//! "end of file" differently.
//!
//! `FuturesOrdered` is the whole trick: it polls every pushed future at once and
//! yields them in the order they were pushed, so out-of-order completions become
//! an in-order stream with no reassembly buffer to keep.

use std::cmp::min;
use std::pin::Pin;

use futures_util::StreamExt;
use futures_util::stream::FuturesOrdered;

/// One chunk read in flight. Boxed so the window is a nameable type; one
/// allocation per chunk.
pub type ChunkFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// `depth` reads of `chunk` bytes in flight over `[start, end)`.
///
/// `read` gets a clone of `reader`, the chunk's offset, and its length, which
/// is `chunk` except for the last one. A plain `fn` keeps the type nameable
/// across a spawn; a backend passes a non-capturing closure.
pub struct ReadWindow<S, T> {
    reader: S,
    read: fn(S, u64, usize) -> ChunkFuture<T>,
    next_offset: u64,
    end: u64,
    depth: usize,
    chunk: usize,
    in_flight: FuturesOrdered<ChunkFuture<T>>,
}

impl<S: Clone, T> ReadWindow<S, T> {
    /// A window over `[start, end)`. A zero `depth` or `chunk` is taken as 1, so
    /// a window always makes progress.
    pub fn new(
        reader: S,
        read: fn(S, u64, usize) -> ChunkFuture<T>,
        start: u64,
        end: u64,
        depth: usize,
        chunk: usize,
    ) -> Self {
        Self {
            reader,
            read,
            next_offset: start,
            end,
            depth: depth.max(1),
            chunk: chunk.max(1),
            in_flight: FuturesOrdered::new(),
        }
    }

    /// The reader every chunk is read through.
    pub fn reader(&self) -> &S {
        &self.reader
    }

    /// The next chunk in file order, or `None` once the range is covered.
    ///
    /// Tops the window back up to `depth` before waiting, so a slow chunk at the
    /// front never leaves the link idle behind it.
    pub async fn next_chunk(&mut self) -> Option<T> {
        while self.in_flight.len() < self.depth && self.next_offset < self.end {
            let offset = self.next_offset;
            let len = min(self.chunk as u64, self.end - offset) as usize;
            self.in_flight.push_back((self.read)(self.reader.clone(), offset, len));
            self.next_offset += len as u64;
        }
        self.in_flight.next().await
    }
}
