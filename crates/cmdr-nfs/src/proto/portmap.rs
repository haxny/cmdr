//! portmap v2 (RFC 1833 § 3): which port a program listens on.

use std::time::Duration;

use crate::proto::rpc::{Procedure, Program, Repeat};
use crate::transport::{CallError, RpcConnection};
use crate::xdr::{Decoder, XdrError};

/// The portmapper itself: program 100000, version 2.
pub(crate) const PORTMAP: Program = Program {
    number: 100_000,
    version: 2,
};

/// Where the portmapper listens when nothing says otherwise.
pub(crate) const PORTMAP_PORT: u16 = 111;

const GETPORT: Procedure = Procedure {
    program: PORTMAP,
    number: 3,
    repeat: Repeat::Safe,
};

/// `IPPROTO_TCP`: this client asks only for TCP ports.
const IPPROTO_TCP: u32 = 6;

/// The TCP port `program` listens on, or `None` when the portmapper has no
/// registration for it (the server doesn't offer that program or version over
/// TCP).
pub(crate) async fn getport(
    conn: &RpcConnection,
    program: Program,
    budget: Option<Duration>,
) -> Result<Option<u16>, CallError> {
    let mut results = conn
        .call(
            GETPORT,
            |enc| {
                enc.u32(program.number);
                enc.u32(program.version);
                enc.u32(IPPROTO_TCP);
                enc.u32(0);
            },
            budget,
        )
        .await?;
    Ok(decode_getport(&mut results)?)
}

/// `GETPORT`'s result: 0 is "not registered", and a number past 65535 is a
/// broken server.
pub(crate) fn decode_getport(results: &mut Decoder) -> Result<Option<u16>, XdrError> {
    match results.u32()? {
        0 => Ok(None),
        port => u16::try_from(port).map(Some).map_err(|_| XdrError::BadDiscriminant {
            what: "port",
            value: port,
        }),
    }
}
