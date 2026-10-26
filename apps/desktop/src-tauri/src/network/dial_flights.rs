//! One dial per saved place, however many callers ask for it at once.
//!
//! A saved place registers its volume only once its dial lands, so the "is it
//! registered already?" check can't guard the 30 s a dial is out: a pane's Enter
//! and an Ask Cmdr `connect_volume` landing in that window would dial twice and
//! register two volumes. Every dial of a saved place goes through here instead
//! (`commands::servers::connect_saved_place`), keyed by the place's volume id:
//!
//! - **A plain caller** (no typed secret, no account override) JOINS a plain dial
//!   already out and gets its outcome.
//! - **A caller carrying a secret** never shares: a plain dial's "needs
//!   credentials" isn't the answer to a password the user just typed. It waits for
//!   whatever dial is out to end, then dials its own (which then sees the volume
//!   registered, if the first landed).
//! - **Each caller cancels only itself** (its attempt id, or dropping its future,
//!   which is how Stop reaches a `connect_volume`). The dial is called off when
//!   the LAST caller stops waiting, so the pane's Cancel never ends a dial the agent
//!   still waits on, nor the other way round.
//!
//! Generic over the outcome so the rules are testable without a server
//! (`dial_flights_test.rs`).

use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Whether a caller may share a dial another caller started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Share {
    /// Stored credentials only: any plain dial to the place answers it.
    Plain,
    /// Carries a typed secret or account: dials on its own, after any dial out.
    Never,
}

/// The dials out right now, by place, and who waits on each.
pub struct DialFlights<T> {
    state: Mutex<State<T>>,
    next_serial: AtomicU64,
}

struct State<T> {
    flights: HashMap<String, Flight<T>>,
    /// Every caller waiting on a dial, by the attempt id its caller minted.
    waiters: HashMap<String, Waiter>,
}

struct Flight<T> {
    serial: u64,
    share: Share,
    /// How many callers still want this dial's outcome.
    waiting: usize,
    /// `None` until the dial ends. Set with `send_replace`, so a caller that
    /// subscribes after the end still reads it.
    outcome: watch::Receiver<Option<T>>,
    /// Fired when the last caller stops waiting.
    call_off: CancellationToken,
}

struct Waiter {
    serial: u64,
    cancel: CancellationToken,
}

/// What a caller does on reaching the place.
enum Step<T> {
    Await(u64, watch::Receiver<Option<T>>),
    /// Another kind of dial is out: wait for it to end, then look again.
    WaitOut(watch::Receiver<Option<T>>),
}

impl<T: Clone + Send + Sync + 'static> DialFlights<T> {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                flights: HashMap::new(),
                waiters: HashMap::new(),
            }),
            next_serial: AtomicU64::new(0),
        }
    }

    /// The outcome of a dial to `place` for the caller `attempt_id`, or `None` when
    /// that caller called itself off ([`Self::cancel`]). `dial` runs only when this
    /// caller starts the flight; it gets the token that fires once nobody waits.
    ///
    /// ❗ The dial runs on its own task, so dropping this future (Stop on an Ask
    /// Cmdr turn) only stops this caller waiting; the flight decides the dial.
    pub async fn dial<F, Fut>(&'static self, place: &str, attempt_id: &str, share: Share, dial: F) -> Option<T>
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
    {
        let cancel = CancellationToken::new();
        let mut dial = Some(dial);
        loop {
            let step = {
                let mut state = self.state.lock_ignore_poison();
                match state.flights.get_mut(place) {
                    // ❗ Not one already called off: it can only answer "cancelled".
                    Some(flight)
                        if share == Share::Plain && flight.share == Share::Plain && !flight.call_off.is_cancelled() =>
                    {
                        flight.waiting += 1;
                        Step::Await(flight.serial, flight.outcome.clone())
                    }
                    Some(flight) => Step::WaitOut(flight.outcome.clone()),
                    None => {
                        let serial = self.next_serial.fetch_add(1, Ordering::Relaxed);
                        let (sender, outcome) = watch::channel(None);
                        let call_off = CancellationToken::new();
                        let start = dial.take().expect("a caller starts at most one flight, then returns");
                        let running = start(call_off.clone());
                        state.flights.insert(
                            place.to_string(),
                            Flight {
                                serial,
                                share,
                                waiting: 1,
                                outcome: outcome.clone(),
                                call_off,
                            },
                        );
                        let place = place.to_string();
                        tokio::spawn(async move {
                            let answer = running.await;
                            sender.send_replace(Some(answer));
                            self.land(&place, serial);
                        });
                        Step::Await(serial, outcome)
                    }
                }
            };
            match step {
                Step::Await(serial, outcome) => {
                    let _waiting = self.wait_on(place, serial, attempt_id, &cancel);
                    return tokio::select! {
                        answer = settled(outcome) => answer,
                        () = cancel.cancelled() => None,
                    };
                }
                Step::WaitOut(outcome) => {
                    let _waiting = self.watch_out(attempt_id, &cancel);
                    tokio::select! {
                        _ = settled(outcome) => {}
                        () = cancel.cancelled() => return None,
                    }
                }
            }
        }
    }

    /// Calls off the caller `attempt_id`, answering whether one was waiting. Its
    /// dial goes on while anyone else waits on it.
    pub fn cancel(&self, attempt_id: &str) -> bool {
        let state = self.state.lock_ignore_poison();
        let Some(waiter) = state.waiters.get(attempt_id) else {
            return false;
        };
        waiter.cancel.cancel();
        true
    }

    /// How many callers wait on the dial to `place` right now.
    #[cfg(test)]
    pub fn waiting_on(&self, place: &str) -> usize {
        self.state
            .lock_ignore_poison()
            .flights
            .get(place)
            .map_or(0, |flight| flight.waiting)
    }

    fn wait_on<'a>(
        &'static self,
        place: &'a str,
        flight: u64,
        attempt_id: &'a str,
        cancel: &CancellationToken,
    ) -> Leave<'a, T> {
        let waiter = self.file_waiter(attempt_id, cancel);
        Leave {
            flights: self,
            place: Some((place, flight)),
            attempt_id,
            waiter,
        }
    }

    fn watch_out<'a>(&'static self, attempt_id: &'a str, cancel: &CancellationToken) -> Leave<'a, T> {
        let waiter = self.file_waiter(attempt_id, cancel);
        Leave {
            flights: self,
            place: None,
            attempt_id,
            waiter,
        }
    }

    fn file_waiter(&self, attempt_id: &str, cancel: &CancellationToken) -> u64 {
        let serial = self.next_serial.fetch_add(1, Ordering::Relaxed);
        self.state.lock_ignore_poison().waiters.insert(
            attempt_id.to_string(),
            Waiter {
                serial,
                cancel: cancel.clone(),
            },
        );
        serial
    }

    /// The flight `serial` ended: the place is free for the next dial.
    fn land(&self, place: &str, serial: u64) {
        let mut state = self.state.lock_ignore_poison();
        if state.flights.get(place).is_some_and(|flight| flight.serial == serial) {
            state.flights.remove(place);
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Default for DialFlights<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The dial's outcome once it has one. `None` only if the dial's task went away
/// without answering (a panic), which reads as called off.
async fn settled<T: Clone>(mut outcome: watch::Receiver<Option<T>>) -> Option<T> {
    match outcome.wait_for(Option::is_some).await {
        Ok(answer) => answer.clone(),
        Err(_) => None,
    }
}

/// One caller's stake in a place, given back however the caller stops waiting:
/// an answer, its own cancel, or its future dropped.
struct Leave<'a, T: Clone + Send + Sync + 'static> {
    flights: &'static DialFlights<T>,
    /// The flight it waits on; `None` while it only waits for another kind out.
    place: Option<(&'a str, u64)>,
    attempt_id: &'a str,
    waiter: u64,
}

impl<T: Clone + Send + Sync + 'static> Drop for Leave<'_, T> {
    fn drop(&mut self) {
        let mut state = self.flights.state.lock_ignore_poison();
        // Only its own entry: a caller reusing the id has replaced it.
        if state
            .waiters
            .get(self.attempt_id)
            .is_some_and(|waiter| waiter.serial == self.waiter)
        {
            state.waiters.remove(self.attempt_id);
        }
        let Some((place, serial)) = self.place else {
            return;
        };
        if let Some(flight) = state.flights.get_mut(place).filter(|flight| flight.serial == serial) {
            flight.waiting = flight.waiting.saturating_sub(1);
            if flight.waiting == 0 {
                flight.call_off.cancel();
            }
        }
    }
}

#[cfg(test)]
#[path = "dial_flights_test.rs"]
mod tests;
