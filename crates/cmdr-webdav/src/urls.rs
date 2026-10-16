//! Turning a remote path into a server URL: the percent-encoding every request
//! URL goes through, whether `transport.rs` sends it or `chunking.rs` plans it.
//! Pure: no `reqwest`.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use url::Url;

/// Everything but the unreserved characters gets encoded in a path segment:
/// spaces, `#`, `?`, `%`, and every other reserved byte included.
pub(crate) const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// `base` with a root-relative remote path like `/Photos/a b.jpg` appended,
/// each segment percent-encoded. `collection` appends the trailing slash a
/// collection is addressed with.
pub(crate) fn join_remote(base: &Url, remote_path: &str, collection: bool) -> Url {
    let mut url = base.clone();
    let mut path = base.path().trim_end_matches('/').to_string();
    for segment in remote_path.split('/').filter(|s| !s.is_empty()) {
        path.push('/');
        path.push_str(&utf8_percent_encode(segment, SEGMENT).to_string());
    }
    if collection || path.is_empty() {
        path.push('/');
    }
    url.set_path(&path);
    url
}
