//! The NFS client's reply decoders (RPC header, portmap, MOUNT, NFSv3) against
//! server-chosen bytes.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| cmdr_nfs::fuzzing::reply(data));
