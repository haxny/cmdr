//! Trying `https` before an `http` address the person typed (#540).
//!
//! Basic auth rides every WebDAV request, so over plain `http` the password
//! crosses the network readable. When someone adds a server by an `http`
//! address, Cmdr first asks whether the same server answers over TLS, and dials
//! that instead when it does. The `http` address is dialed as typed only when
//! `https` doesn't answer, and the sheet's warning stays up for it.

use std::future::Future;

use tokio_util::sync::CancellationToken;
use url::Url;

use crate::params::WebdavConnectionParams;
use crate::transport;

impl WebdavConnectionParams {
    /// These params, moved to their `https` twin when that answers over TLS
    /// with a certificate this Mac trusts. ❗ The probe sends no credentials: it
    /// runs before the password has anywhere safe to go.
    ///
    /// A method rather than a root export: the crate's public surface is capped
    /// (`DETAILS.md` § "The public surface is capped").
    pub async fn preferring_https(self, cancel: &CancellationToken) -> Self {
        prefer_https_with(self, |url| transport::answers_over_tls(url, cancel)).await
    }
}

/// [`prefer_https`] over an injected probe, so the choice is testable without
/// a server holding a trusted certificate.
async fn prefer_https_with<F, Fut>(params: WebdavConnectionParams, answers: F) -> WebdavConnectionParams
where
    F: FnOnce(Url) -> Fut,
    Fut: Future<Output = bool>,
{
    match params.https_twin() {
        Some(twin) if answers(twin.base_url.clone()).await => twin,
        _ => params,
    }
}

#[cfg(test)]
#[path = "https_first_test.rs"]
mod https_first_test;
