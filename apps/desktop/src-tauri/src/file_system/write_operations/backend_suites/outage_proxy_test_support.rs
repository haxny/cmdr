//! A TCP proxy a test stands between a network volume and its Docker fixture,
//! so it can take the network away and give it back: the outage `#398` is
//! about, staged without touching the shared container stack.
//!
//! ❌ Never stop or pause a fixture CONTAINER for this. The stacks are leased
//! machine-wide (`scripts/check/CLAUDE.md` § fixture stacks), so a stopped
//! container fails every other worktree's suite that happens to be running.
//! A proxy of the test's own breaks only the test's own connections.
//!
//! Protocol-blind: it forwards bytes. [`OutageProxy::cut`] drops every live
//! connection (the client sees its socket close, the way a router restart
//! looks) and refuses new ones until [`OutageProxy::restore`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::ignore_poison::IgnorePoison;

pub(crate) struct OutageProxy {
    port: u16,
    shared: Arc<Shared>,
    listener: JoinHandle<()>,
}

struct Shared {
    up: AtomicBool,
    live: Mutex<Vec<JoinHandle<()>>>,
}

impl OutageProxy {
    /// Starts forwarding from a fresh `127.0.0.1` port to `127.0.0.1:target_port`.
    pub(crate) async fn start(target_port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("binding the outage proxy to a loopback port");
        let port = listener.local_addr().expect("the proxy's own address").port();
        let shared = Arc::new(Shared {
            up: AtomicBool::new(true),
            live: Mutex::new(Vec::new()),
        });
        let accepting = Arc::clone(&shared);
        let listener = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                if !accepting.up.load(Ordering::SeqCst) {
                    // The network is down: the connection opens and closes at
                    // once, the way a dial into a dead route fails.
                    drop(client);
                    continue;
                }
                let forwarding = tokio::spawn(forward(client, target_port));
                accepting.live.lock_ignore_poison().push(forwarding);
            }
        });
        Self { port, shared, listener }
    }

    /// The loopback port to connect through.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// The network goes away: every live connection closes, and new ones are
    /// refused until [`restore`](Self::restore).
    pub(crate) fn cut(&self) {
        self.shared.up.store(false, Ordering::SeqCst);
        for connection in self.shared.live.lock_ignore_poison().drain(..) {
            connection.abort();
        }
    }

    /// The network is back: new connections forward again.
    pub(crate) fn restore(&self) {
        self.shared.up.store(true, Ordering::SeqCst);
    }
}

impl Drop for OutageProxy {
    fn drop(&mut self) {
        self.listener.abort();
        for connection in self.shared.live.lock_ignore_poison().drain(..) {
            connection.abort();
        }
    }
}

async fn forward(mut client: TcpStream, target_port: u16) {
    let Ok(mut server) = TcpStream::connect(("127.0.0.1", target_port)).await else {
        return;
    };
    // Ends when either side closes, or when `cut` aborts this task, which drops
    // both sockets.
    let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
}
