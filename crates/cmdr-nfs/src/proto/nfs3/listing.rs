//! A whole directory listing: READDIRPLUS page after page, or READDIR and a
//! LOOKUP per entry where the server has no READDIRPLUS, every entry handed
//! back with its handle and attributes.

use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::stream;
use log::debug;

use super::{DirEntry, DirEntryPlus, Fattr3, FileHandle, Nfs3Client, NfsError, NfsStat3};

/// How many times a listing restarts on `NFS3ERR_BAD_COOKIE` before giving up.
const LISTING_RESTARTS: u32 = 3;
/// How many LOOKUPs (or GETATTRs) a listing keeps in flight to fill entries
/// the server listed without a handle or attributes.
const LOOKUPS_IN_FLIGHT: usize = 16;

/// One entry of a whole listing: name, handle, and attributes, all present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListedEntry {
    /// The name, as the server's bytes.
    pub(crate) name: Bytes,
    /// Its handle.
    pub(crate) handle: FileHandle,
    /// Its attributes (a symlink's own).
    pub(crate) attributes: Fattr3,
}

/// What [`Nfs3Client::list_dir`] hands its caller as the listing goes.
#[derive(Debug)]
pub(crate) enum ListingEvent {
    /// Another batch of entries.
    Entries(Vec<ListedEntry>),
    /// The server invalidated the listing (`BAD_COOKIE`); it starts again from
    /// the top. ❗ Discard every entry delivered so far.
    Restarted,
}

impl Nfs3Client {
    /// The whole listing of `dir`, a page at a time, every entry with its
    /// handle and attributes.
    ///
    /// - READDIRPLUS from cookie 0 with a zero `cookieverf`, echoing the
    ///   returned verifier on every later page. `NOTSUPP` on the first page
    ///   falls back to READDIR + a LOOKUP per entry.
    /// - An entry READDIRPLUS sent without a handle costs a LOOKUP, and one
    ///   without attributes a GETATTR (pipelined, [`LOOKUPS_IN_FLIGHT`] at a
    ///   time), ❌ never a skipped or attribute-less row. One that's gone by the
    ///   time it's looked up is left out: it no longer exists.
    /// - `BAD_COOKIE` restarts from the top (telling `on_event`
    ///   [`ListingEvent::Restarted`]), at most [`LISTING_RESTARTS`] times.
    ///
    /// Answers the folder's own attributes as the FIRST page of the listing
    /// that completed carried them (`None` when the server sent none): a
    /// change during a later page moves the folder's stamp past this one, so a
    /// poller comparing against it re-lists rather than missing the change.
    pub(crate) async fn list_dir(
        &self,
        dir: &FileHandle,
        page_bytes: u32,
        mut on_event: impl FnMut(ListingEvent),
    ) -> Result<Option<Fattr3>, NfsError> {
        let mut plus = true;
        let mut restarts = 0;
        'listing: loop {
            let mut cookie = 0;
            let mut cookieverf = [0u8; 8];
            let mut folder = None;
            loop {
                let (entries, next_cookie, next_verf, eof) = if plus {
                    match self.readdirplus(dir, cookie, cookieverf, page_bytes).await {
                        Ok(page) => {
                            if cookie == 0 {
                                folder = page.dir_attributes;
                            }
                            let entries = self.complete_plus(dir, page.entries).await?;
                            (entries, page.next_cookie, page.cookieverf, page.eof)
                        }
                        Err(NfsError::Status(NfsStat3::NotSupp)) if cookie == 0 => {
                            debug!(target: "nfs", "the server has no READDIRPLUS; listing with READDIR and LOOKUP");
                            plus = false;
                            continue;
                        }
                        Err(NfsError::Status(NfsStat3::BadCookie)) if restarts < LISTING_RESTARTS => {
                            restarts += 1;
                            on_event(ListingEvent::Restarted);
                            continue 'listing;
                        }
                        Err(err) => return Err(err),
                    }
                } else {
                    match self.readdir(dir, cookie, cookieverf, page_bytes).await {
                        Ok(page) => {
                            if cookie == 0 {
                                folder = page.dir_attributes;
                            }
                            let entries = self.complete_plain(dir, page.entries).await?;
                            (entries, page.next_cookie, page.cookieverf, page.eof)
                        }
                        Err(NfsError::Status(NfsStat3::BadCookie)) if restarts < LISTING_RESTARTS => {
                            restarts += 1;
                            on_event(ListingEvent::Restarted);
                            continue 'listing;
                        }
                        Err(err) => return Err(err),
                    }
                };
                if !entries.is_empty() {
                    on_event(ListingEvent::Entries(entries));
                }
                if eof {
                    return Ok(folder);
                }
                match next_cookie {
                    Some(next) => cookie = next,
                    None => return Err(NfsError::NoProgress),
                }
                cookieverf = next_verf;
            }
        }
    }

    /// Fills each READDIRPLUS entry's missing handle or attributes.
    async fn complete_plus(&self, dir: &FileHandle, entries: Vec<DirEntryPlus>) -> Result<Vec<ListedEntry>, NfsError> {
        let filled: Vec<Result<Option<ListedEntry>, NfsError>> = stream::iter(entries)
            .map(|entry| async move {
                match (entry.handle, entry.attributes) {
                    (Some(handle), Some(attributes)) => Ok(Some(ListedEntry {
                        name: entry.name,
                        handle,
                        attributes,
                    })),
                    (Some(handle), None) => match self.getattr(&handle).await {
                        Ok(attributes) => Ok(Some(ListedEntry {
                            name: entry.name,
                            handle,
                            attributes,
                        })),
                        Err(NfsError::Status(NfsStat3::NoEnt | NfsStat3::Stale)) => Ok(None),
                        Err(err) => Err(err),
                    },
                    (None, _) => self.lookup_entry(dir, entry.name).await,
                }
            })
            .buffered(LOOKUPS_IN_FLIGHT)
            .collect()
            .await;
        filled.into_iter().filter_map(Result::transpose).collect()
    }

    /// LOOKUPs every READDIR entry, pipelined.
    async fn complete_plain(&self, dir: &FileHandle, entries: Vec<DirEntry>) -> Result<Vec<ListedEntry>, NfsError> {
        let filled: Vec<Result<Option<ListedEntry>, NfsError>> = stream::iter(entries)
            .map(|entry| self.lookup_entry(dir, entry.name))
            .buffered(LOOKUPS_IN_FLIGHT)
            .collect()
            .await;
        filled.into_iter().filter_map(Result::transpose).collect()
    }

    /// One listed name's handle and attributes, or `None` when it's gone.
    async fn lookup_entry(&self, dir: &FileHandle, name: Bytes) -> Result<Option<ListedEntry>, NfsError> {
        let found = match self.lookup(dir, &name).await {
            Ok(found) => found,
            Err(NfsError::Status(NfsStat3::NoEnt)) => return Ok(None),
            Err(err) => return Err(err),
        };
        let attributes = match found.attributes {
            Some(attributes) => attributes,
            None => match self.getattr(&found.handle).await {
                Ok(attributes) => attributes,
                Err(NfsError::Status(NfsStat3::NoEnt | NfsStat3::Stale)) => return Ok(None),
                Err(err) => return Err(err),
            },
        };
        Ok(Some(ListedEntry {
            name,
            handle: found.handle,
            attributes,
        }))
    }
}
