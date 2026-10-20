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

use crate::errors::WebdavConnectError;
use crate::params::WebdavConnectionParams;
use crate::transport;

impl WebdavConnectionParams {
    /// Dials these params with `dial`, moved to their `https` twin when that
    /// answers as WebDAV over TLS with a certificate this Mac trusts. ❗ The
    /// probe sends no credentials: it runs before the password has anywhere
    /// safe to go.
    ///
    /// A method rather than a root export: the crate's public surface is capped
    /// (`DETAILS.md` § "The public surface is capped").
    pub async fn dial_preferring_https<T, D, Fut>(
        self,
        cancel: &CancellationToken,
        dial: D,
    ) -> Result<T, WebdavConnectError>
    where
        D: FnMut(WebdavConnectionParams) -> Fut,
        Fut: Future<Output = Result<T, WebdavConnectError>>,
    {
        dial_preferring_https_with(self, |url| transport::answers_as_webdav(url, cancel), cancel, dial).await
    }
}

/// [`WebdavConnectionParams::dial_preferring_https`] over an injected probe, so
/// the choice is testable without a server holding a trusted certificate.
///
/// ❗ A twin that answered the probe but whose dial didn't come up falls back to
/// the typed address, whatever the reason (a 443 serving another site, a root
/// or account the twin doesn't have): the probe only says something answers
/// there, ❌ never that it's the server the person typed. Cancel is the one
/// exception, by the error or by the token: it means stop.
async fn dial_preferring_https_with<T, P, PFut, D, Fut>(
    params: WebdavConnectionParams,
    answers: P,
    cancel: &CancellationToken,
    mut dial: D,
) -> Result<T, WebdavConnectError>
where
    P: FnOnce(Url) -> PFut,
    PFut: Future<Output = bool>,
    D: FnMut(WebdavConnectionParams) -> Fut,
    Fut: Future<Output = Result<T, WebdavConnectError>>,
{
    let Some(twin) = params.https_twin() else {
        return dial(params).await;
    };
    let answered = answers(twin.base_url.clone()).await;
    if cancel.is_cancelled() {
        return Err(WebdavConnectError::Cancelled);
    }
    if !answered {
        return dial(params).await;
    }
    match dial(twin).await {
        Err(WebdavConnectError::Cancelled) => Err(WebdavConnectError::Cancelled),
        Err(refusal) if cancel.is_cancelled() => Err(refusal),
        Err(refusal) => {
            log::info!(target: "volume", "the https twin answered but didn't connect ({refusal:?}); dialing http as typed");
            dial(params).await
        }
        connected => connected,
    }
}

#[cfg(test)]
#[path = "https_first_test.rs"]
mod https_first_test;
