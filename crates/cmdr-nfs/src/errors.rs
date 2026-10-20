//! Why a connection attempt didn't produce a working volume.

/// Why dialing an NFS server, listing its exports, or mounting one didn't work.
///
/// ❗ Every variant comes from a NUMBER the server sent (`mountstat3`,
/// `accept_stat`, `reject_stat`, `auth_stat`) or from the transport, ❌ never
/// from a message. The `String`s are log-only diagnostics, ❌ never shown and
/// never branched on.
#[derive(Debug)]
pub enum NfsConnectError {
    /// The server couldn't be reached: DNS, refused, no route, or a connection
    /// that broke during the dial.
    Unreachable(String),
    /// The dial didn't finish inside its budget.
    TimedOut,
    /// The server offers no NFSv3 over TCP (portmap lists none, or a call
    /// answered `PROG_MISMATCH`). ❌ Never a fallback to another version.
    NoNfsV3,
    /// No export by that path on this server (`MNT3ERR_NOENT`).
    ExportNotFound,
    /// The server refused this client the export (`MNT3ERR_ACCES`, an `AUTH_*`
    /// rejection, or a mount whose flavors don't include `AUTH_SYS`), while the
    /// request came from a reserved port.
    ExportRefused,
    /// The server won't list its exports to this client (MOUNT's `EXPORT`
    /// refused). ❗ Distinct from [`Unreachable`](Self::Unreachable): the server
    /// is there, and an export typed by hand may still mount.
    ListingRefused,
    /// The export's refusal, while this process couldn't bind a source port below
    /// 1024: a `secure` export refuses every client that doesn't, so the fix is
    /// the port, ❌ not the export.
    ReservedPortRequired,
    /// Anything else: a reply that wouldn't decode, a status no other variant
    /// covers.
    Transport(String),
    /// The user called it off. Nothing is wrong with the server.
    Cancelled,
}
