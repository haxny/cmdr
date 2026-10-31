//! ONC RPC v2 (RFC 5531): the call header every request opens with, the reply
//! header every answer opens with, and the `AUTH_SYS` credential.
//!
//! ❗ Every outcome here is a NUMBER the server sent (`accept_stat`,
//! `reject_stat`, `auth_stat`), kept as a typed value; an unknown number is kept
//! as itself, ❌ never folded into a neighbour.

use bytes::Bytes;

use crate::xdr::{Decoder, Encoder, XdrError};

/// The RPC protocol version this client speaks (RFC 5531 § 8).
const RPC_VERSION: u32 = 2;
/// `msg_type`: a call.
const CALL: u32 = 0;
/// `msg_type`: a reply.
pub(crate) const REPLY: u32 = 1;
/// The `AUTH_NONE` flavor: no credential, and the verifier every call sends.
const AUTH_NONE: u32 = 0;
/// The `AUTH_SYS` flavor (`AUTH_UNIX` in older texts).
pub(crate) const AUTH_SYS: u32 = 1;
/// RFC 5531 caps any credential or verifier body at 400 bytes.
const MAX_AUTH_BODY: usize = 400;
/// `AUTH_SYS` caps the machine name at 255 bytes.
const MAX_MACHINE_NAME: usize = 255;
/// `AUTH_SYS` caps the supplementary group list at 16.
pub(crate) const MAX_GIDS: usize = 16;

/// One remote program and version: what a connection talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Program {
    /// The program number (100003 for NFS).
    pub(crate) number: u32,
    /// The version (3 for NFSv3 and MOUNT v3).
    pub(crate) version: u32,
}

/// Whether a call may go out twice. The transport resends a stuck call only
/// when it's [`Repeat::Safe`].
///
/// ❗ A resent `CREATE` or `REMOVE` that the server already did answers `EXIST`
/// or `NOENT`, which reads as "the name is taken" or "already gone": wrong
/// answers that look right, so those are [`Repeat::Never`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Repeat {
    /// Doing it twice is doing it once (`GETATTR`, `READ`, `WRITE` at an offset).
    Safe,
    /// A second copy can change the answer (`CREATE`, `REMOVE`, `RENAME`).
    Never,
}

/// One procedure of a [`Program`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Procedure {
    /// The program the procedure belongs to.
    pub(crate) program: Program,
    /// The procedure number.
    pub(crate) number: u32,
    /// Whether a stuck call may be resent.
    pub(crate) repeat: Repeat,
}

/// Who a call says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Credential {
    /// `AUTH_NONE`: the portmapper and the liveness probe ask nothing more.
    None,
    /// `AUTH_SYS`, its body encoded once.
    Sys(Bytes),
}

/// The fields of an `AUTH_SYS` credential (RFC 5531 Appendix A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthSys {
    /// An arbitrary id the server may use to tell credentials apart.
    pub(crate) stamp: u32,
    /// This machine's name. Cut to 255 bytes (on a character boundary) when
    /// longer.
    pub(crate) machine_name: String,
    /// The user id every call acts as.
    pub(crate) uid: u32,
    /// The primary group id.
    pub(crate) gid: u32,
    /// The supplementary groups, at most 16 (more are dropped).
    pub(crate) gids: Vec<u32>,
}

impl Credential {
    /// An `AUTH_SYS` credential, its body encoded and held to RFC 5531's caps.
    pub(crate) fn sys(auth: &AuthSys) -> Self {
        let mut name_len = auth.machine_name.len().min(MAX_MACHINE_NAME);
        while !auth.machine_name.is_char_boundary(name_len) {
            name_len -= 1;
        }
        let gids = &auth.gids[..auth.gids.len().min(MAX_GIDS)];
        let mut enc = Encoder::new();
        enc.u32(auth.stamp);
        enc.opaque(&auth.machine_name.as_bytes()[..name_len]);
        enc.u32(auth.uid);
        enc.u32(auth.gid);
        enc.u32(gids.len() as u32);
        for gid in gids {
            enc.u32(*gid);
        }
        // At most 4 + (4 + 256) + 4 + 4 + 4 + 16 * 4 = 340 bytes: inside
        // RFC 5531's 400-byte credential cap by construction.
        Self::Sys(enc.finish().freeze())
    }
}

/// Writes a call header (RFC 5531 § 9): everything before the procedure's
/// arguments. The verifier is always `AUTH_NONE`.
pub(crate) fn encode_call_header(enc: &mut Encoder, xid: u32, procedure: Procedure, credential: &Credential) {
    enc.u32(xid);
    enc.u32(CALL);
    enc.u32(RPC_VERSION);
    enc.u32(procedure.program.number);
    enc.u32(procedure.program.version);
    enc.u32(procedure.number);
    match credential {
        Credential::None => {
            enc.u32(AUTH_NONE);
            enc.opaque(&[]);
        }
        Credential::Sys(body) => {
            enc.u32(AUTH_SYS);
            enc.opaque(body);
        }
    }
    enc.u32(AUTH_NONE);
    enc.opaque(&[]);
}

/// Why the server refused to run a call it understood the framing of
/// (`accept_stat`, RFC 5531 § 9, other than `SUCCESS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AcceptError {
    /// `PROG_UNAVAIL`: nothing by that program number here.
    ProgramUnavailable,
    /// `PROG_MISMATCH`: the program is here, in versions `low..=high` only.
    ProgramMismatch {
        /// The lowest version the server speaks.
        low: u32,
        /// The highest.
        high: u32,
    },
    /// `PROC_UNAVAIL`: the program has no such procedure.
    ProcedureUnavailable,
    /// `GARBAGE_ARGS`: the arguments wouldn't decode on the server.
    GarbageArgs,
    /// `SYSTEM_ERR`: the server failed on its own side (out of memory).
    SystemError,
    /// A number RFC 5531 doesn't define.
    Unknown(u32),
}

/// `auth_stat` (RFC 5531 § 9): why the server refused a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthStat {
    /// `AUTH_BADCRED`: the credential wouldn't decode or was malformed.
    BadCredential,
    /// `AUTH_REJECTEDCRED`: the server wants a new session.
    RejectedCredential,
    /// `AUTH_BADVERF`.
    BadVerifier,
    /// `AUTH_REJECTEDVERF`.
    RejectedVerifier,
    /// `AUTH_TOOWEAK`: refused for its security (classically, a non-reserved
    /// source port).
    TooWeak,
    /// Any other number, `AUTH_OK` included (a denial saying "ok" is nonsense).
    Other(u32),
}

impl AuthStat {
    fn from_number(number: u32) -> Self {
        match number {
            1 => Self::BadCredential,
            2 => Self::RejectedCredential,
            3 => Self::BadVerifier,
            4 => Self::RejectedVerifier,
            5 => Self::TooWeak,
            other => Self::Other(other),
        }
    }
}

/// Why the server denied a call outright (`reject_stat`, RFC 5531 § 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rejection {
    /// `RPC_MISMATCH`: the server doesn't speak RPC v2.
    RpcMismatch {
        /// The lowest RPC version it speaks.
        low: u32,
        /// The highest.
        high: u32,
    },
    /// `AUTH_ERROR`: the credential was refused.
    Auth(AuthStat),
}

/// A reply's header, decoded: the call it answers, and either the results or
/// why there are none.
#[derive(Debug)]
pub(crate) enum ReplyBody {
    /// `MSG_ACCEPTED` + `SUCCESS`: the procedure's results follow.
    Success(Decoder),
    /// `MSG_ACCEPTED` with any other `accept_stat`.
    NotAccepted(AcceptError),
    /// `MSG_DENIED`.
    Denied(Rejection),
}

/// The XID a record opens with, if it's long enough to have one.
pub(crate) fn peek_xid(record: &Bytes) -> Option<u32> {
    record
        .get(..4)
        .map(|word| u32::from_be_bytes([word[0], word[1], word[2], word[3]]))
}

/// Decodes a reply header (RFC 5531 § 9) off `record`.
pub(crate) fn decode_reply(record: Bytes) -> Result<(u32, ReplyBody), XdrError> {
    let mut dec = Decoder::new(record);
    let xid = dec.u32()?;
    let msg_type = dec.u32()?;
    if msg_type != REPLY {
        return Err(XdrError::BadDiscriminant {
            what: "msg_type",
            value: msg_type,
        });
    }
    let body = match dec.u32()? {
        0 => {
            let _flavor = dec.u32()?;
            let _verifier = dec.opaque(MAX_AUTH_BODY)?;
            match dec.u32()? {
                0 => ReplyBody::Success(dec),
                1 => ReplyBody::NotAccepted(AcceptError::ProgramUnavailable),
                2 => ReplyBody::NotAccepted(AcceptError::ProgramMismatch {
                    low: dec.u32()?,
                    high: dec.u32()?,
                }),
                3 => ReplyBody::NotAccepted(AcceptError::ProcedureUnavailable),
                4 => ReplyBody::NotAccepted(AcceptError::GarbageArgs),
                5 => ReplyBody::NotAccepted(AcceptError::SystemError),
                other => ReplyBody::NotAccepted(AcceptError::Unknown(other)),
            }
        }
        1 => match dec.u32()? {
            0 => ReplyBody::Denied(Rejection::RpcMismatch {
                low: dec.u32()?,
                high: dec.u32()?,
            }),
            1 => ReplyBody::Denied(Rejection::Auth(AuthStat::from_number(dec.u32()?))),
            value => {
                return Err(XdrError::BadDiscriminant {
                    what: "reject_stat",
                    value,
                });
            }
        },
        value => {
            return Err(XdrError::BadDiscriminant {
                what: "reply_stat",
                value,
            });
        }
    };
    Ok((xid, body))
}

#[cfg(test)]
#[path = "rpc_test.rs"]
mod rpc_test;
