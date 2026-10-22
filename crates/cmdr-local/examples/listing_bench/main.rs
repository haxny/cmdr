//! Prices moving the local listing to `getattrlistbulk`, and what "Date added"
//! and Finder's "Last opened" cost per entry. Dev-only evidence for
//! `docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md`; never shipped.
//! macOS only.
//!
//! ```sh
//! cargo run --release -p cmdr-local --example listing_bench -- gen <dir> 100000
//! cargo run --release -p cmdr-local --example listing_bench -- bench <dir> [runs] [methods]
//! cargo run --release -p cmdr-local --example listing_bench -- parity <dir>...
//! cargo run --release -p cmdr-local --example listing_bench -- volcaps <dir>...
//! cargo run --release -p cmdr-local --example listing_bench -- vanish <dir>
//! ```
//!
//! Methods (`bench`'s third argument, comma-separated; default: the first four):
//! - `current`: `reading::list_directory_core`, what production runs.
//! - `bulk`: the `getattrlistbulk` prototype, same fields plus `added_at`.
//! - `bulk_xattr`: `bulk` plus one `getxattr(com.apple.lastuseddate#PS)` per file.
//! - `current_added`: `current` plus one `getattrlist(ATTR_CMN_ADDEDTIME)` per entry.
//! - `nsurl`: `reading::list_directory`, the NSURL per-file enrichment the app has today.
//!
//! `bench` interleaves methods round-robin (one warm-up round, then `runs`
//! rounds) so none of them gets a systematically warmer cache, and reports the
//! median wall and CPU (user + sys, `getrusage`) per method. `runs` = 0 times
//! one pass per method and nothing else: give each method its own fresh fixture
//! and process for the cold-ish numbers.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a measurement harness prints its measurements"
)]

#[cfg(target_os = "macos")]
mod bulk;

#[cfg(target_os = "macos")]
fn main() {
    macos::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("listing_bench is macOS only");
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::CString;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use cmdr_fs::entry::FileEntry;
    use cmdr_fs::volume::host::listings::RecordingListings;
    use cmdr_local::reading;

    use super::bulk::{self, BulkOptions};

    const DEFAULT_BUF: usize = 128 * 1024;

    pub fn main() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let Some(cmd) = args.first() else {
            eprintln!("usage: listing_bench gen|bench|parity|volcaps|vanish ...");
            std::process::exit(2);
        };
        match cmd.as_str() {
            "gen" => generate(Path::new(&args[1]), args[2].parse().expect("file count")),
            "bench" => {
                let runs = args.get(2).map_or(7, |r| r.parse().expect("runs"));
                let methods = args
                    .get(3)
                    .map_or("current,bulk,bulk_xattr,current_added", String::as_str);
                bench(Path::new(&args[1]), runs, methods);
            }
            "parity" => {
                for dir in &args[1..] {
                    parity(Path::new(dir));
                }
            }
            "volcaps" => {
                for dir in &args[1..] {
                    volcaps(Path::new(dir));
                }
            }
            "vanish" => vanish(Path::new(&args[1])),
            "dump" => bulk::dump(Path::new(&args[1]), 4).expect("dump"),
            other => panic!("unknown command {other}"),
        }
    }

    fn buf_bytes() -> usize {
        std::env::var("BULK_BUF_KIB")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map_or(DEFAULT_BUF, |k| k * 1024)
    }

    fn opts(added_time: bool, last_used_xattr: bool) -> BulkOptions {
        BulkOptions {
            added_time,
            last_used_xattr,
            buf_bytes: buf_bytes(),
        }
    }

    fn run_method(method: &str, dir: &Path, host: &RecordingListings) -> Vec<FileEntry> {
        match method {
            "current" => reading::list_directory_core(dir, host).expect("current"),
            "bulk" => bulk::list(dir, host, opts(true, false)).expect("bulk"),
            "bulk_xattr" => bulk::list(dir, host, opts(true, true)).expect("bulk_xattr"),
            "current_added" => {
                let mut entries = reading::list_directory_core(dir, host).expect("current");
                for entry in &mut entries {
                    entry.added_at = bulk::added_time_by_path(Path::new(&entry.path));
                }
                entries
            }
            "nsurl" => reading::list_directory(dir, host).expect("nsurl"),
            other => panic!("unknown method {other}"),
        }
    }

    fn cpu_time() -> Duration {
        // SAFETY: `getrusage` fills the zeroed struct we hand it.
        let usage = unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut usage);
            usage
        };
        let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
        tv(usage.ru_utime) + tv(usage.ru_stime)
    }

    fn median(mut v: Vec<Duration>) -> Duration {
        v.sort();
        v[v.len() / 2]
    }

    fn bench(dir: &Path, runs: usize, methods: &str) {
        let host = RecordingListings::new();
        let methods: Vec<&str> = methods.split(',').collect();
        let mut wall: Vec<Vec<Duration>> = vec![Vec::new(); methods.len()];
        let mut cpu: Vec<Vec<Duration>> = vec![Vec::new(); methods.len()];
        let mut first: Vec<Option<(Duration, Duration)>> = vec![None; methods.len()];
        let mut count = 0;
        for round in 0..=runs {
            for (i, method) in methods.iter().enumerate() {
                let c0 = cpu_time();
                let t0 = Instant::now();
                let entries = run_method(method, dir, &host);
                let w = t0.elapsed();
                let c = cpu_time() - c0;
                count = entries.len();
                std::hint::black_box(&entries);
                drop(entries);
                if round == 0 {
                    first[i] = Some((w, c));
                } else {
                    wall[i].push(w);
                    cpu[i].push(c);
                }
            }
        }
        println!(
            "dir={} entries={count} runs={runs} buf={}KiB",
            dir.display(),
            buf_bytes() / 1024
        );
        for (i, method) in methods.iter().enumerate() {
            let (fw, fc) = first[i].expect("warm-up ran");
            if runs == 0 {
                println!(
                    "  {method:<14} first pass only: wall {:>9.2} ms  cpu {:>9.2} ms  ({:.2} µs/entry)",
                    fw.as_secs_f64() * 1e3,
                    fc.as_secs_f64() * 1e3,
                    fw.as_secs_f64() * 1e6 / count.max(1) as f64,
                );
                continue;
            }
            let mw = median(wall[i].clone());
            let mc = median(cpu[i].clone());
            println!(
                "  {method:<14} median wall {:>9.2} ms  cpu {:>9.2} ms  ({:.2} µs/entry)  | first pass wall {:>9.2} ms cpu {:>9.2} ms",
                mw.as_secs_f64() * 1e3,
                mc.as_secs_f64() * 1e3,
                mw.as_secs_f64() * 1e6 / count.max(1) as f64,
                fw.as_secs_f64() * 1e3,
                fc.as_secs_f64() * 1e3,
            );
        }
    }

    /// Diffs the bulk prototype against the current lister, field for field,
    /// with the two new dates set aside (they're checked separately: `added_at`
    /// against a per-path `getattrlist`).
    fn parity(dir: &Path) {
        let host = RecordingListings::new();
        let current = match reading::list_directory_core(dir, &host) {
            Ok(e) => e,
            Err(e) => {
                println!("{}: current failed: {e}", dir.display());
                return;
            }
        };
        let bulk = match bulk::list(dir, &host, opts(true, true)) {
            Ok(e) => e,
            Err(e) => {
                println!("{}: bulk failed: {e} (current listed {})", dir.display(), current.len());
                return;
            }
        };
        let by_name = |v: &[FileEntry]| {
            let mut m: Vec<FileEntry> = v.to_vec();
            m.sort_by(|a, b| a.name.cmp(&b.name));
            m
        };
        let (c, b) = (by_name(&current), by_name(&bulk));
        let mut diffs = 0;
        let mut added_some = 0;
        let mut added_mismatch = 0;
        let mut opened_some = 0;
        if c.len() != b.len() {
            println!("{}: COUNT current={} bulk={}", dir.display(), c.len(), b.len());
        }
        let bulk_names: std::collections::HashMap<&str, &FileEntry> = b.iter().map(|e| (e.name.as_str(), e)).collect();
        for ce in &c {
            let Some(be) = bulk_names.get(ce.name.as_str()) else {
                println!("  only in current: {}", ce.name);
                diffs += 1;
                continue;
            };
            if be.added_at.is_some() {
                added_some += 1;
            }
            if be.opened_at.is_some() {
                opened_some += 1;
            }
            if be.added_at != bulk::added_time_by_path(Path::new(&ce.path)) {
                added_mismatch += 1;
            }
            let mut stripped = (*be).clone();
            stripped.added_at = None;
            stripped.opened_at = None;
            let (cs, bs) = (format!("{ce:?}"), format!("{stripped:?}"));
            if cs != bs {
                diffs += 1;
                if diffs <= 8 {
                    let fields: Vec<String> = cs
                        .split(", ")
                        .zip(bs.split(", "))
                        .filter(|(a, b)| a != b)
                        .map(|(a, b)| format!("{a} vs {b}"))
                        .collect();
                    println!("  DIFF {} (current vs bulk): {}", ce.name, fields.join("; "));
                }
            }
        }
        println!(
            "{}: {} entries, {diffs} differing, added_at set on {added_some} ({added_mismatch} disagree with per-path getattrlist), lastuseddate xattr on {opened_some}, {} mount/firmlink re-stats",
            dir.display(),
            c.len(),
            bulk::FALLBACKS.swap(0, std::sync::atomic::Ordering::Relaxed),
        );
    }

    fn volcaps(dir: &Path) {
        match bulk::volume_attr_support(dir) {
            Ok(s) => println!(
                "{}: attrlist native={}, ADDEDTIME valid={} native={}, FLAGS valid={}",
                dir.display(),
                s.attrlist_native,
                s.addedtime_valid,
                s.addedtime_native,
                s.flags_valid
            ),
            Err(e) => println!("{}: volcaps failed: {e}", dir.display()),
        }
    }

    /// Lists with a tiny buffer, deleting every not-yet-returned `vanish-*`
    /// file after the first batch, to see what `getattrlistbulk` does when
    /// entries disappear mid-listing.
    fn vanish(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir).expect("mkdir");
        for i in 0..2000 {
            fs::write(dir.join(format!("vanish-{i:05}")), b"").expect("write");
        }
        let c_path = CString::new(dir.as_os_str().as_bytes()).expect("cstring");
        // SAFETY: valid NUL-terminated path.
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
        assert!(fd >= 0, "open");
        let mut al = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_NAME | 0x2000_0000,
            volattr: 0,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        let mut buf = vec![0u64; 512];
        let mut total = 0;
        let mut calls = 0;
        loop {
            // SAFETY: `al` valid, `buf` writable for 4 KiB.
            let n = unsafe {
                libc::getattrlistbulk(
                    fd,
                    (&raw mut al).cast(),
                    buf.as_mut_ptr().cast(),
                    4096,
                    u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
                )
            };
            if n < 0 {
                println!("call {calls}: error {}", std::io::Error::last_os_error());
                break;
            }
            if n == 0 {
                break;
            }
            calls += 1;
            total += n;
            if calls == 1 {
                // Delete everything; the kernel already handed us the first batch.
                for i in 0..2000 {
                    let _ = fs::remove_file(dir.join(format!("vanish-{i:05}")));
                }
                println!("first batch {n}, then deleted all 2000");
            }
        }
        // SAFETY: closing the fd opened above.
        unsafe { libc::close(fd) };
        println!("vanish: {calls} calls, {total} entries returned in total");
    }

    /// Builds a fixture: `files` files (90% empty, 10% with 1–8 KiB), plus
    /// directories, symlinks (to files, to dirs, broken), hard links, `UF_HIDDEN`
    /// and dot files, and the last-used xattr on 30% of files (39 of 99 files
    /// carried it in a real Downloads folder).
    fn generate(dir: &Path, files: usize) {
        fs::create_dir_all(dir).expect("mkdir");
        let dirs = (files / 30).clamp(30, 300);
        let links = dirs;
        let payload = vec![b'x'; 8192];
        for i in 0..files {
            let name = if i % 100 == 7 {
                format!(".dot-{i:06}")
            } else {
                format!("file-{i:06}.txt")
            };
            let p = dir.join(&name);
            let len = if i % 10 == 3 { 1024 + (i % 8) * 896 } else { 0 };
            fs::write(&p, &payload[..len]).expect("write");
            if i % 100 == 11 {
                set_hidden_flag(&p);
            }
            if i % 10 < 3 {
                set_last_used(&p, 1_780_000_000 + i as i64);
            }
        }
        for i in 0..dirs {
            fs::create_dir(dir.join(format!("dir-{i:04}"))).expect("mkdir");
        }
        for i in 0..links {
            let target = match i % 3 {
                0 => PathBuf::from(format!("file-{:06}.txt", i * 3)),
                1 => PathBuf::from(format!("dir-{:04}", i % dirs)),
                _ => PathBuf::from(format!("missing-{i}")),
            };
            symlink(target, dir.join(format!("link-{i:04}"))).expect("symlink");
        }
        for i in 0..20.min(files) {
            let src = dir.join(format!("file-{:06}.txt", i * 10 + 1));
            let _ = fs::hard_link(&src, dir.join(format!("hardlink-{i:02}")));
        }
        println!(
            "{}: {files} files, {dirs} dirs, {links} symlinks, 20 hard links",
            dir.display()
        );
    }

    fn set_hidden_flag(p: &Path) {
        let c = CString::new(p.as_os_str().as_bytes()).expect("cstring");
        // SAFETY: valid NUL-terminated path.
        unsafe { libc::chflags(c.as_ptr(), libc::UF_HIDDEN) };
    }

    fn set_last_used(p: &Path, secs: i64) {
        let c = CString::new(p.as_os_str().as_bytes()).expect("cstring");
        let mut value = [0u8; 16];
        value[..8].copy_from_slice(&secs.to_le_bytes());
        // SAFETY: valid NUL-terminated strings; `value` readable for 16 bytes.
        unsafe {
            libc::setxattr(
                c.as_ptr(),
                bulk::LAST_USED_XATTR.as_ptr().cast(),
                value.as_ptr().cast(),
                16,
                0,
                0,
            )
        };
    }
}
