//! The three ONC RPC programs an NFSv3 client speaks, each as typed calls:
//! `rpc` (the call and reply headers, `AUTH_SYS`), `portmap` (`GETPORT`),
//! `mount` (`MNT`, `UMNT`, `EXPORT`), and `nfs3` (every procedure the volume
//! uses). Everything above this speaks typed procedure calls, ❌ never bytes.

pub(crate) mod mount;
pub(crate) mod nfs3;
pub(crate) mod portmap;
pub(crate) mod rpc;
#[cfg(test)]
pub(crate) mod server;

#[cfg(test)]
#[path = "knfsd_replies_test.rs"]
mod knfsd_replies_test;
