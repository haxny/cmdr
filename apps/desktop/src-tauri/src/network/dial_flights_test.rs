//! One dial per saved place: shared by plain callers, never by one carrying a
//! secret, and called off only when the last caller stops waiting.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::test_support::wait_until_async;

fn flights() -> &'static DialFlights<&'static str> {
    Box::leak(Box::new(DialFlights::new()))
}

/// A dial that counts itself, waits for `release`, and answers `outcome`. It
/// hands its call-off token to `called_off` so a test can watch it.
struct Script {
    dials: Arc<AtomicUsize>,
    release: Arc<Notify>,
    called_off: Arc<Mutex<Vec<CancellationToken>>>,
}

impl Script {
    fn new() -> Self {
        Self {
            dials: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Notify::new()),
            called_off: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn dial(
        &self,
        outcome: &'static str,
    ) -> impl FnOnce(CancellationToken) -> futures_util::future::BoxFuture<'static, &'static str> + Send + 'static {
        let (dials, release, called_off) = (self.dials.clone(), self.release.clone(), self.called_off.clone());
        move |call_off| {
            Box::pin(async move {
                dials.fetch_add(1, Ordering::SeqCst);
                called_off.lock().expect("test lock").push(call_off.clone());
                tokio::select! {
                    () = release.notified() => outcome,
                    () = call_off.cancelled() => "called off",
                }
            })
        }
    }

    fn dials(&self) -> usize {
        self.dials.load(Ordering::SeqCst)
    }
}

async fn until(what: &str, check: impl Fn() -> bool) {
    wait_until_async(Duration::from_secs(5), what, check).await;
}

#[tokio::test]
async fn two_plain_callers_share_one_dial_and_its_outcome() {
    let flights = flights();
    let script = Script::new();

    let first = tokio::spawn(flights.dial("nas", "pane", Share::Plain, script.dial("connected")));
    until("the first dial set out", || script.dials() == 1).await;
    let second = tokio::spawn(flights.dial("nas", "agent", Share::Plain, script.dial("a second dial")));
    until("the agent joined", || flights.waiting_on("nas") == 2).await;
    script.release.notify_waiters();

    assert_eq!(first.await.expect("joins"), Some("connected"));
    assert_eq!(second.await.expect("joins"), Some("connected"));
    assert_eq!(script.dials(), 1, "one place, one dial");
}

/// A sign-in sheet's typed password must reach a dial of its own: sharing a
/// plain dial would answer it "needs credentials" while it holds one.
#[tokio::test]
async fn a_caller_with_a_secret_waits_for_the_plain_dial_then_dials_its_own() {
    let flights = flights();
    let script = Script::new();

    let plain = tokio::spawn(flights.dial("nas", "agent", Share::Plain, script.dial("needs credentials")));
    until("the plain dial set out", || script.dials() == 1).await;
    let sheet = tokio::spawn(flights.dial("nas", "sheet", Share::Never, script.dial("connected")));
    tokio::task::yield_now().await;
    assert_eq!(script.dials(), 1, "the sheet's dial waits for the one out");

    script.release.notify_one();
    assert_eq!(plain.await.expect("joins"), Some("needs credentials"));
    until("the sheet's own dial set out", || script.dials() == 2).await;
    script.release.notify_one();
    assert_eq!(sheet.await.expect("joins"), Some("connected"));
}

/// The pane's Cancel lets the pane go; the agent still waiting keeps the dial.
#[tokio::test]
async fn one_caller_calling_off_leaves_the_dial_to_the_others() {
    let flights = flights();
    let script = Script::new();

    let pane = tokio::spawn(flights.dial("nas", "pane", Share::Plain, script.dial("connected")));
    until("the dial set out", || script.dials() == 1).await;
    let agent = tokio::spawn(flights.dial("nas", "agent", Share::Plain, script.dial("unused")));
    until("the agent joined", || flights.waiting_on("nas") == 2).await;

    assert!(flights.cancel("pane"));
    assert_eq!(pane.await.expect("joins"), None, "the pane stops waiting at once");
    assert!(!script.called_off.lock().expect("test lock")[0].is_cancelled());

    script.release.notify_waiters();
    assert_eq!(agent.await.expect("joins"), Some("connected"));
}

/// Stop drops the agent's call; with nobody else waiting, the dial is called off.
#[tokio::test]
async fn the_last_caller_leaving_calls_the_dial_off() {
    let flights = flights();
    let script = Script::new();

    let agent = tokio::spawn(flights.dial("nas", "agent", Share::Plain, script.dial("connected")));
    until("the dial set out", || script.dials() == 1).await;
    agent.abort();
    let _ = agent.await;

    let call_off = script.called_off.lock().expect("test lock")[0].clone();
    until("the dial was called off", || call_off.is_cancelled()).await;
    until("the place is free again", || flights.waiting_on("nas") == 0).await;
}

#[tokio::test]
async fn a_cancel_for_an_attempt_nobody_runs_is_a_plain_no() {
    assert!(!flights().cancel("never-filed"));
}
