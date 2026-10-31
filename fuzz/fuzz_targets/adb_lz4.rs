//! The LZ4 frame reader behind a compressed ADB pull, against device-chosen bytes.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| cmdr_adb::fuzzing::lz4_frame(data));
