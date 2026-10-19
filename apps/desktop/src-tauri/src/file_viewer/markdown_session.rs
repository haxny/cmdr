//! The rendered-Markdown path for the file viewer.
//!
//! A `.md` file (by extension, under [`MARKDOWN_RENDER_CAP_BYTES`]) opens as a
//! *Markdown session*: like a media session it has no text backend and empty text
//! fields, so the frontend's media branch shows it, and "View as text" reopens it as a
//! plain text session. Unlike a media session it keeps the file it opened, on whatever
//! volume (local, a share over smb2, a pulled temp), because the render reads through
//! it: [`render_markdown_session`] reads the whole document, renders it, and sanitizes
//! it (`markdown_render.rs`), minting a deferred `cmdr-media://` token per image
//! (`markdown_images.rs`). The open stays as quick as any other; the render is its own
//! call, and the images load after it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;

use super::content_kind::{
    CLASSIFY_HEAD_LEN, MARKDOWN_RENDER_CAP_BYTES, ViewerContentKind, is_markdown_extension, is_renderable_markdown,
};
use super::encoding::{DETECT_HEAD_BYTES, FileEncoding, decode_line, detect_from_head};
use super::markdown_images::{ImageTarget, RemoteImage, VolumeImage, resolve_image_src};
use super::markdown_links::{MarkdownLink, resolve_link};
use super::markdown_render::{ImageMarker, render_markdown};
use super::materialize::PreviewTemp;
use super::media;
use super::media_backend::MediaBackend;
use super::origin::SourceOrigin;
use super::session_store::{
    BackendType, SESSIONS, ViewerOpenResult, ViewerSession, ViewerSessionInit, generate_session_id,
};
use super::source::{OpenedFile, read_head};
use super::{FileViewerBackend, LineChunk, ViewerError};

/// What the viewer shows for a Markdown session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownRender {
    /// Sanitized HTML. Images carry a `data-md-image` marker instead of a `src`:
    /// `local:<token>`, `remote:<token>`, or `blocked` (`markdown_render.rs`).
    pub html: String,
    /// Whether any image is on the web, so the view offers to load them.
    pub has_remote_images: bool,
}

/// A Markdown session's document: where it lives, and the image tokens its render minted.
pub(super) struct MarkdownDocument {
    /// The document's path on its volume (never a temp copy's), which relative image
    /// sources resolve against.
    path: PathBuf,
    volume_id: String,
    /// Set when the session closes, so an image pull still running stops.
    closed: Arc<AtomicBool>,
    /// Set by the user's "Load remote images"; until then no web image is fetched.
    remote_allowed: Arc<AtomicBool>,
    /// The tokens the last render minted, by what they point at, so a repeated image
    /// shares one token (and one pull).
    images: Mutex<HashMap<ImageTarget, String>>,
}

impl MarkdownDocument {
    fn new(path: PathBuf, volume_id: String) -> Self {
        Self {
            path,
            volume_id,
            closed: Arc::new(AtomicBool::new(false)),
            remote_allowed: Arc::new(AtomicBool::new(false)),
            images: Mutex::new(HashMap::new()),
        }
    }

    /// What an image `src` becomes, minting its token on first sight.
    fn image_marker(&self, src: &str, volume_root: &Path) -> ImageMarker {
        let target = resolve_image_src(src, &self.path, volume_root);
        let mut images = self.images.lock_ignore_poison();
        let token = |images: &mut HashMap<ImageTarget, String>, source: Arc<dyn media::DeferredMedia>| {
            images
                .entry(target.clone())
                .or_insert_with(|| media::mint_deferred_token(source))
                .clone()
        };
        match &target {
            ImageTarget::Local(path) => {
                let image = VolumeImage::new(path.clone(), self.volume_id.clone(), Arc::clone(&self.closed));
                ImageMarker::Local(token(&mut images, Arc::new(image)))
            }
            ImageTarget::Remote(url) => {
                let image = RemoteImage::new(url.clone(), Arc::clone(&self.remote_allowed), Arc::clone(&self.closed));
                ImageMarker::Remote(token(&mut images, Arc::new(image)))
            }
            ImageTarget::Inline => ImageMarker::Inline,
            ImageTarget::Blocked => ImageMarker::Blocked,
        }
    }

    fn has_remote_images(&self) -> bool {
        self.images
            .lock_ignore_poison()
            .keys()
            .any(|target| matches!(target, ImageTarget::Remote(_)))
    }

    /// Drops every image token, so none outlives the render it came from.
    fn drop_images(&self) {
        for (_, token) in self.images.lock_ignore_poison().drain() {
            media::drop_token(&token);
        }
    }

    /// The session closed: stop the pulls and drop the tokens. Called from the single
    /// `close_session` choke point, like a media session's token drop.
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.drop_images();
    }
}

/// Whether the file `opened` from `requested` opens in the rendered Markdown view. Reads
/// the head only for a file with a Markdown name: every open passes through here.
pub(super) fn is_markdown(requested: &Path, opened: &OpenedFile) -> bool {
    let ext = requested.extension().and_then(|e| e.to_str());
    if !is_markdown_extension(ext) {
        return false;
    }
    let head = read_head(opened.source.as_ref(), opened.total_bytes, CLASSIFY_HEAD_LEN).unwrap_or_default();
    is_renderable_markdown(&head, ext, opened.total_bytes)
}

/// What a Markdown open hands over: the file as the text path would have opened it.
pub(super) struct MarkdownOpen {
    /// The path the viewer was asked for, on `volume_id`.
    pub(super) requested: PathBuf,
    pub(super) volume_id: String,
    /// What the session reads: `requested`, or the temp copy of it.
    pub(super) file_path: PathBuf,
    pub(super) origin: SourceOrigin,
    pub(super) opened: OpenedFile,
    pub(super) temp: Option<Arc<PreviewTemp>>,
}

/// Opens a Markdown session over a file [`is_markdown`] accepted. No read beyond the
/// head the classification took: the render is [`render_markdown_session`].
pub(super) fn open_markdown_session(open: MarkdownOpen) -> ViewerOpenResult {
    let file_size = open.opened.total_bytes;
    let file_name = open.opened.file_name.clone();
    let backend: Box<dyn FileViewerBackend> = Box::new(MediaBackend::new(file_name.clone(), file_size));
    let capabilities = backend.capabilities();
    let session_id = generate_session_id();
    // Like a media session: no watcher, no line index, placeholder encoding fields.
    let session = ViewerSession::new(ViewerSessionInit {
        backend,
        backend_type: BackendType::FullLoad,
        upgrading: None,
        encoding: FileEncoding::Utf8,
        detected_encoding: FileEncoding::Utf8,
        watcher_stop: Arc::new(AtomicBool::new(false)),
        path: open.file_path,
        origin: open.origin,
        file: Some(open.opened),
        media_token: None,
        temp: open.temp,
        markdown: Some(Arc::new(MarkdownDocument::new(open.requested, open.volume_id))),
    });
    SESSIONS.lock_ignore_poison().insert(session_id.clone(), session);

    ViewerOpenResult {
        session_id,
        file_name,
        total_bytes: file_size,
        total_lines: Some(0),
        estimated_total_rows: 0,
        backend_type: BackendType::FullLoad,
        capabilities,
        initial_lines: LineChunk {
            rows: Vec::new(),
            first_row_number: 0,
            byte_offset: 0,
            end_byte_offset: 0,
            end: super::ChunkEnd::EndOfFile,
            total_rows: super::TotalRows::Exact(0),
            total_bytes: file_size,
        },
        is_indexing: false,
        encoding: FileEncoding::Utf8,
        kind: ViewerContentKind::Markdown,
        media_token: None,
        media_dimensions: None,
    }
}

/// The session's document as sanitized HTML. Reads the whole file through the session's
/// own handle (a local file, a share over smb2, or a pulled temp), so it costs one read
/// of at most [`MARKDOWN_RENDER_CAP_BYTES`]. A second render replaces the first one's
/// image tokens.
pub fn render_markdown_session(session_id: &str) -> Result<MarkdownRender, ViewerError> {
    let (document, opened) = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        let document = markdown_of(session, session_id)?;
        (document, session.open_file()?)
    };
    let text = read_document(&opened)?;
    let volume_root = volume_root(&document.volume_id);

    document.drop_images();
    let for_images = Arc::clone(&document);
    let html = render_markdown(&text, move |src| for_images.image_marker(src, &volume_root));
    if document.closed.load(Ordering::Relaxed) {
        // The window closed mid-render, after `close` already dropped the tokens.
        document.drop_images();
    }
    Ok(MarkdownRender {
        html,
        has_remote_images: document.has_remote_images(),
    })
}

/// Where a link clicked in this session's rendered document goes (`markdown_links.rs`).
/// No I/O: a link to a file that isn't there is the open's to report.
pub fn resolve_markdown_link(session_id: &str, href: &str) -> Result<MarkdownLink, ViewerError> {
    let document = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        markdown_of(session, session_id)?
    };
    Ok(resolve_link(href, &document.path, &volume_root(&document.volume_id)))
}

/// The root of the volume a document lives on, which its relative references can't climb
/// past. The local drive's `/` when the volume is gone.
fn volume_root(volume_id: &str) -> PathBuf {
    get_volume_manager()
        .get(volume_id)
        .map_or_else(|| PathBuf::from("/"), |volume| volume.root().to_path_buf())
}

/// Lets this session's document load its web images. Until it's called, a remote
/// image's token answers 404 and nothing goes over the network.
pub fn allow_remote_images(session_id: &str) -> Result<(), ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;
    markdown_of(session, session_id)?
        .remote_allowed
        .store(true, Ordering::Relaxed);
    Ok(())
}

/// The session's Markdown document. A session without one is, as far as a Markdown
/// call is concerned, not there.
fn markdown_of(session: &ViewerSession, session_id: &str) -> Result<Arc<MarkdownDocument>, ViewerError> {
    session.markdown.clone().ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })
}

/// The whole document, decoded the way the text view would decode it.
fn read_document(opened: &OpenedFile) -> Result<String, ViewerError> {
    let len = opened.total_bytes.min(MARKDOWN_RENDER_CAP_BYTES) as usize;
    let mut bytes = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        let read = opened.source.read_at(filled as u64, &mut bytes[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    bytes.truncate(filled);
    let encoding = detect_from_head(&bytes[..bytes.len().min(DETECT_HEAD_BYTES)]);
    let body = bytes.strip_prefix(encoding.bom_bytes()).unwrap_or(&bytes);
    Ok(decode_line(body, encoding))
}
