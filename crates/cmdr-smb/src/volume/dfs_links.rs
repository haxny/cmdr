//! Following a DFS link inside the share for the transfer calls, which drive
//! smb2's lock-free `Tree` API on a cloned connection, and that API doesn't
//! follow links.
//!
//! A share can hold an `msdfs:` link to a folder on another server (smb2's
//! `smb-dfs-root` fixture is one). smb2 follows a link on every `SmbClient`
//! call that takes a path (vdavid/smb2#15), but those take `&mut SmbClient`, so
//! they'd hold the session's client for the whole transfer. So every transfer
//! call runs on the `Tree` first, the way it always has, and only a CREATE the
//! server refuses with `STATUS_PATH_NOT_COVERED` (the one answer that means
//! "behind a link", [`is_behind_dfs_link`]) runs again through the `SmbClient`,
//! holding the client for the OPEN only: smb2 resolves the referral, connects
//! the target (pooled on that client), and hands back a handle that owns the
//! target's connection and tree.
//!
//! - **Writes** open a `FileWriter` there (`'static`), or send the one compound
//!   frame there, and set the dates through the same client.
//! - **Reads** open a `FileReader` there and stream it with [`LinkDownload`]'s
//!   fixed window of positioned reads. smb2's `SmbClient::download` borrows the
//!   client for the download's life, which would hold the session's client
//!   (the main one's would stall browsing) for as long as the file takes, so the
//!   link path trades the adaptive read-ahead for a fixed window. A link is the
//!   rare case; an owned link download in smb2 would let this go.
//! - **A server-side copy through a link** answers `NotSupported` (`copy.rs`),
//!   and the engine streams the file through the paths above.
//! - The caller's tree never moves (smb2's own rule): the next path on it still
//!   means the caller's share.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::stream::FuturesOrdered;

use cmdr_fs::volume::WriteMode;
use smb2::SmbClient;
use smb2::client::stream::{FileReader, FileWriter};
use smb2::client::tree::Tree;

use super::SmbVolumeInner;
use super::session::{LeaseOn, MainSession};
use super::state::RanOn;

/// Whether `err` is a server saying the path sits behind a DFS link: a CREATE
/// refused with `STATUS_PATH_NOT_COVERED`, as smb2's own trigger reads it. A
/// refused CREATE wrote nothing, so running the call again through the link is
/// safe.
pub(super) fn is_behind_dfs_link(err: &smb2::Error) -> bool {
    matches!(
        err,
        smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::PATH_NOT_COVERED,
            command: smb2::types::Command::Create,
        }
    )
}

/// The client that owns a lease's session, locked to follow a link.
pub(super) enum LinkClient<'a> {
    Main(tokio::sync::MutexGuard<'a, Option<MainSession>>),
    Transfer(tokio::sync::OwnedMutexGuard<SmbClient>),
}

impl LinkClient<'_> {
    pub(super) fn client(&mut self) -> &mut SmbClient {
        match self {
            Self::Main(guard) => &mut guard.as_mut().expect("checked at lock").client,
            Self::Transfer(guard) => guard,
        }
    }
}

impl SmbVolumeInner {
    /// Locks the client a lease's session belongs to, or `None` when that
    /// session is no longer installed (a reconnect replaced it: the lease's tree
    /// would be refused on the new client, so the caller keeps its error).
    pub(super) async fn link_client(&self, ran_on: RanOn, on: &LeaseOn) -> Option<LinkClient<'_>> {
        match (on, ran_on) {
            (LeaseOn::Main, RanOn::Main(generation)) => {
                let guard = self.session.lock().await;
                guard
                    .as_ref()
                    .is_some_and(|s| s.generation == generation)
                    .then_some(LinkClient::Main(guard))
            }
            (LeaseOn::Transfer { member, .. }, RanOn::Transfer(_)) => {
                Some(LinkClient::Transfer(Arc::clone(&member.client).lock_owned().await))
            }
            _ => None,
        }
    }

    /// Opens `smb_path` for reading through the DFS link it sits behind.
    pub(super) async fn open_link_reader(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
    ) -> Option<Result<FileReader, smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        Some(client.client().open_file_reader(tree, smb_path).await)
    }

    /// Opens the streaming writer `mode` asks for through the DFS link
    /// `smb_path` sits behind. The writer owns the target's connection, so the
    /// client is held for the open only.
    pub(super) async fn open_link_writer(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
        mode: WriteMode,
    ) -> Option<Result<FileWriter, smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        let client = client.client();
        Some(match mode {
            WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => {
                client.create_file_writer_exclusive(tree, smb_path).await
            }
            WriteMode::CreateOrReplace => client.create_file_writer(tree, smb_path).await,
        })
    }

    /// The one-frame write `mode` asks for, through the DFS link `smb_path`
    /// sits behind. Holds the client for the frame's two round trips.
    pub(super) async fn link_write_compound(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
        data: &[u8],
        mode: WriteMode,
    ) -> Option<Result<u64, smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        let client = client.client();
        Some(match mode {
            WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => {
                client.write_file_compound_exclusive(tree, smb_path, data).await
            }
            WriteMode::CreateOrReplace => client.write_file_compound(tree, smb_path, data).await,
        })
    }

    /// Sets `times` by path through the DFS link `smb_path` sits behind.
    pub(super) async fn link_set_times(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
        times: smb2::FileTimes,
    ) -> Option<Result<(), smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        Some(client.client().set_times(tree, smb_path, times).await)
    }

    /// Deletes `smb_path` through the DFS link it sits behind, for a failed
    /// write's partial file.
    pub(super) async fn link_delete_file(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
    ) -> Option<Result<(), smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        Some(client.client().delete_file(tree, smb_path).await)
    }
}

/// How many positioned reads a link download keeps in flight, each one
/// `smb2::DOWNLOAD_CHUNK_SIZE` (512 KiB): 2 MiB on the wire, half of what
/// smb2's adaptive window grows to on a fast link, and a fixed queue ahead of
/// anything else on the target's connection.
const LINK_READ_WINDOW: usize = 4;

type ChunkRead = Pin<Box<dyn Future<Output = Result<Vec<u8>, smb2::Error>> + Send>>;

/// A streaming read of a file behind a DFS link: [`LINK_READ_WINDOW`]
/// positioned reads in flight on one open `FileReader`, delivered in order.
pub(super) struct LinkDownload {
    reader: Arc<FileReader>,
    size: u64,
    next_offset: u64,
    received: u64,
    in_flight: FuturesOrdered<ChunkRead>,
}

impl LinkDownload {
    pub(super) fn new(reader: FileReader) -> Self {
        let size = reader.size();
        Self {
            reader: Arc::new(reader),
            size,
            next_offset: 0,
            received: 0,
            in_flight: FuturesOrdered::new(),
        }
    }

    pub(super) fn size(&self) -> u64 {
        self.size
    }

    pub(super) fn info(&self) -> &smb2::FileInfo {
        self.reader.info()
    }

    pub(super) fn bytes_received(&self) -> u64 {
        self.received
    }

    /// The next chunk, in file order; `None` at the end (a file that shrank
    /// ends early).
    pub(super) async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, smb2::Error>> {
        let chunk = smb2::DOWNLOAD_CHUNK_SIZE as u64;
        while self.in_flight.len() < LINK_READ_WINDOW && self.next_offset < self.size {
            let reader = Arc::clone(&self.reader);
            let offset = self.next_offset;
            let len = chunk.min(self.size - offset);
            self.in_flight
                .push_back(Box::pin(async move { reader.read_at(offset, len).await }));
            self.next_offset += len;
        }
        match self.in_flight.next().await? {
            Ok(bytes) if bytes.is_empty() => {
                self.in_flight = FuturesOrdered::new();
                self.next_offset = self.size;
                None
            }
            Ok(bytes) => {
                self.received += bytes.len() as u64;
                Some(Ok(bytes))
            }
            Err(e) => {
                self.in_flight = FuturesOrdered::new();
                self.next_offset = self.size;
                Some(Err(e))
            }
        }
    }

    /// Closes the handle once nothing is in flight. Dropping the download
    /// instead closes it too (smb2 queues the CLOSE on the reader's drop).
    pub(super) async fn close(mut self) -> Result<(), smb2::Error> {
        self.in_flight = FuturesOrdered::new();
        match Arc::try_unwrap(self.reader) {
            Ok(reader) => reader.close().await,
            Err(_) => Ok(()),
        }
    }
}
