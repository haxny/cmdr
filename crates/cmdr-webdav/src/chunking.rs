//! Nextcloud's chunked upload: where it lives and how a file splits for it.
//!
//! A reverse proxy in front of a Nextcloud commonly caps one request's body
//! (Cloudflare at 100 MB, an nginx at whatever `client_max_body_size` says), so
//! a big file sent as ONE `PUT` fails after the user waited through most of
//! it. Nextcloud's own answer is its chunking API ("Chunked file upload" in its
//! developer manual): `MKCOL` a collection under `remote.php/dav/uploads/<user
//! id>/`, `PUT` the file into it as numbered chunks, then `MOVE` the
//! collection's `.file` onto the destination, which assembles them. Version 2
//! (the recommended one) wants a `Destination` header on all three; a server
//! without v2 (no distributed cache, or older than 26) ignores it and assembles
//! the same chunks the v1 way, so one request shape serves both.
//!
//! Pure: no `reqwest`. `volume/writes.rs` sends the requests. `DETAILS.md` §
//! "Chunked uploads (Nextcloud)".

use percent_encoding::utf8_percent_encode;
use url::Url;

use crate::urls::{SEGMENT, join_remote};

/// The chunk a big file is cut into, and the size past which a file goes in
/// chunks at all (a file that fits in one chunk goes as one `PUT`).
///
/// 64 MiB: under Cloudflare's 100 MB request cap (its Free and Pro plans), the
/// tightest common proxy limit, and far above Nextcloud v2's 5 MB floor for
/// every chunk but the last. A request per 64 MiB costs nothing measurable
/// next to the bytes.
pub(crate) const CHUNK_SIZE: u64 = 64 * 1024 * 1024;

/// The most chunks one upload may have: v2 takes chunk names 1 to 10,000.
const MAX_CHUNKS: u64 = 10_000;

/// A Nextcloud's chunked-upload home for one account, and how to name a
/// destination so its uploads collection accepts it.
#[derive(Debug)]
pub(crate) struct ChunkedUploads {
    /// `<install>/remote.php/dav/uploads/<user id>/`.
    uploads: Url,
    /// The base URL as the `dav/files` endpoint spells it. ❗ A `Destination`
    /// outside `remote.php/dav` is refused (sabre/dav resolves it against its
    /// own base), so the legacy `remote.php/webdav/` is re-spelled here.
    files: Url,
}

impl ChunkedUploads {
    /// The chunked-upload home behind `base`, when the server is a Nextcloud:
    /// it named a user id (`X-User-Id` on the probe's answer) and `base` is one
    /// of its account endpoints (`remote.php/webdav/…` or
    /// `remote.php/dav/files/<user>/…`). `None` for everything else, which
    /// uploads in one piece: another server, a public share, the DAV root.
    pub(crate) fn detect(base: &Url, user_id: Option<&str>) -> Option<Self> {
        let user_id = user_id.filter(|id| !id.is_empty())?;
        let user = utf8_percent_encode(user_id, SEGMENT).to_string();
        let segments: Vec<&str> = base.path().split('/').filter(|s| !s.is_empty()).collect();
        let at = segments.iter().position(|&s| s == "remote.php")?;
        let install = segments[..at].iter().map(|s| format!("/{s}")).collect::<String>();
        let files_path = match segments.get(at + 1..)? {
            ["webdav", rest @ ..] => {
                let rest = rest.iter().map(|s| format!("/{s}")).collect::<String>();
                format!("{install}/remote.php/dav/files/{user}{rest}/")
            }
            ["dav", "files", _owner, ..] => base.path().to_string(),
            _ => return None,
        };
        let mut uploads = base.clone();
        uploads.set_path(&format!("{install}/remote.php/dav/uploads/{user}/"));
        let mut files = base.clone();
        files.set_path(&files_path);
        Some(Self { uploads, files })
    }

    /// The uploads collection `id`, addressed as a collection.
    pub(crate) fn session(&self, id: &str) -> Url {
        join_remote(&self.uploads, id, true)
    }

    /// Chunk `number` (1-based) of upload `id`.
    pub(crate) fn chunk(&self, id: &str, number: u32) -> Url {
        join_remote(&self.uploads, &format!("{id}/{}", chunk_name(number)), false)
    }

    /// The assembled file of upload `id`, which a `MOVE` puts in place.
    pub(crate) fn assembled(&self, id: &str) -> Url {
        join_remote(&self.uploads, &format!("{id}/.file"), false)
    }

    /// The `Destination` for a root-relative remote path.
    pub(crate) fn destination(&self, remote_path: &str) -> Url {
        join_remote(&self.files, remote_path, false)
    }
}

/// A chunk's name: its number, zero-padded so names sort as numbers do (v1
/// assembles in name order, v2 by the number).
pub(crate) fn chunk_name(number: u32) -> String {
    format!("{number:05}")
}

/// The chunks a file of `size` bytes goes up in: `(number, length)`, numbered
/// from 1, every one `preferred` long but the last. A file too big for 10,000
/// of them gets longer chunks instead.
pub(crate) fn chunks(size: u64, preferred: u64) -> impl Iterator<Item = (u32, u64)> {
    let len = preferred.max(size.div_ceil(MAX_CHUNKS)).max(1);
    (1u32..)
        .zip((0..size).step_by(usize::try_from(len).unwrap_or(usize::MAX)))
        .map(move |(number, offset)| (number, len.min(size - offset)))
}

#[cfg(test)]
#[path = "chunking_test.rs"]
mod chunking_test;
