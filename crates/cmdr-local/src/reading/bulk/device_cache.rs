//! Which devices list in bulk, remembered per device, so a re-stat answers
//! `get_single_entry`'s "does my folder list in bulk?" from the `lstat` it
//! already made: no `statfs` per call. Write paths re-stat constantly
//! (preflight, cleanup, conflict checks), and on an smbfs destination a
//! `statfs` can reach the server.
//!
//! An entry lives on its folder's filesystem, so the entry's own device
//! answers for the folder, with one exception: a mount point, the root of
//! another filesystem. Each device remembers its root's inode, and a folder
//! with that inode falls back to the parent `statfs` it always took.
//!
//! ❗ A device number is reused after an unmount (DiskArbitration hands a freed
//! unit to the next disk at once), so the cache forgets every device on any
//! filesystem event. A thread blocks on a kqueue `EVFILT_FS` filter, which the
//! kernel signals inside `mount` and `unmount` themselves, and counts events in
//! an atomic each lookup reads. ❌ Don't poll the kqueue inline instead: a
//! zero-timeout `kevent` costs ~12 µs of wall time per call (it yields), more
//! than the whole re-stat (`docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md`
//! § "What a re-stat costs").

use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr};
use std::fs;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{bulk_eligible, parent_is_bulk_eligible};

/// What a device answers, read once per device between filesystem events.
#[derive(Clone, Copy)]
struct Device {
    /// The device's filesystem lists in bulk.
    eligible: bool,
    /// The inode of the device's mount root, the one folder on it whose
    /// listing is on ANOTHER device.
    root_ino: u64,
}

impl Device {
    /// The answer for the entry `metadata` describes, or `None` for the mount
    /// root, whose folder is on another filesystem.
    fn holder_answer(self, metadata: &fs::Metadata) -> Option<bool> {
        let is_mount_root = metadata.is_dir() && metadata.ino() == self.root_ino;
        (!is_mount_root).then_some(self.eligible)
    }
}

struct Cache {
    /// The filesystem event count `devices` is valid for.
    events_seen: u64,
    devices: HashMap<u64, Device>,
}

impl Cache {
    /// Forgets every device if a filesystem event arrived since the last look,
    /// and returns the event count the devices are now valid for.
    fn catch_up(&mut self) -> u64 {
        let events = FILESYSTEM_EVENTS.load(Ordering::Acquire);
        if events != self.events_seen {
            self.devices.clear();
            self.events_seen = events;
        }
        events
    }
}

/// Filesystem events the watcher thread has woken for.
static FILESYSTEM_EVENTS: AtomicU64 = AtomicU64::new(0);

/// Set if the watcher thread stopped; nothing is cached after that.
static WATCHER_STOPPED: AtomicBool = AtomicBool::new(false);

static CACHE: OnceLock<Option<Mutex<Cache>>> = OnceLock::new();

/// The process's cache, or `None` when the kernel wouldn't give us the mount
/// events that keep it honest (then every call asks `statfs`).
fn cache() -> Option<&'static Mutex<Cache>> {
    CACHE
        .get_or_init(|| {
            watch_filesystem_events().then(|| {
                Mutex::new(Cache {
                    events_seen: FILESYSTEM_EVENTS.load(Ordering::Acquire),
                    devices: HashMap::new(),
                })
            })
        })
        .as_ref()
        .filter(|_| !WATCHER_STOPPED.load(Ordering::Acquire))
}

/// Registers a kqueue for `EVFILT_FS` (mounts, unmounts, and every other
/// filesystem-wide event) and starts the thread that counts them. `false` if
/// the kernel refused either. The registration lands before this returns, so
/// the thread can't miss an event that follows.
fn watch_filesystem_events() -> bool {
    // SAFETY: `kqueue` takes no arguments and returns a new fd or -1.
    let raw = unsafe { libc::kqueue() };
    if raw < 0 {
        return false;
    }
    // SAFETY: `raw` is a fresh fd nothing else owns.
    let kq = unsafe { OwnedFd::from_raw_fd(raw) };
    let change = libc::kevent {
        ident: 0,
        filter: libc::EVFILT_FS,
        flags: libc::EV_ADD | libc::EV_CLEAR,
        fflags: 0,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    // SAFETY: `kq` is open; `change` is one valid registration; no event
    // buffer is passed (null, 0), so the kernel writes nothing back.
    let rc = unsafe { libc::kevent(kq.as_raw_fd(), &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    if rc != 0 {
        return false;
    }
    std::thread::Builder::new()
        .name("cmdr-fs-events".to_string())
        .stack_size(64 * 1024)
        .spawn(move || count_filesystem_events(&kq))
        .is_ok()
}

/// Blocks on `kq` for the life of the process, counting every wake. An
/// unexpected error stops the thread and, with it, the caching.
fn count_filesystem_events(kq: &OwnedFd) {
    // SAFETY: `kevent` is plain old data (integers and a null pointer), so
    // all-zero bytes are a valid value of it.
    let mut events: [libc::kevent; 8] = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: `kq` is open for this thread's lifetime; no changes are passed
        // (null, 0); `events` is writable for the count passed; a null timeout
        // blocks until an event arrives.
        let woke = unsafe {
            libc::kevent(
                kq.as_raw_fd(),
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                events.len() as libc::c_int,
                std::ptr::null(),
            )
        };
        // An error can't tell us nothing changed, so it counts too.
        FILESYSTEM_EVENTS.fetch_add(1, Ordering::AcqRel);
        if woke < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            WATCHER_STOPPED.store(true, Ordering::Release);
            return;
        }
    }
}

/// Whether the folder holding `path` lists in bulk, given `path`'s own `lstat`
/// (`metadata`). Equal to [`parent_is_bulk_eligible`], which it falls back to
/// for a mount point and for a device it can't read.
pub(in crate::reading) fn holder_is_bulk_eligible(path: &Path, metadata: &fs::Metadata) -> bool {
    let Some(cache) = cache() else {
        return parent_is_bulk_eligible(path);
    };
    let dev = metadata.dev();
    let events = {
        let mut cache = cache.lock_ignore_poison();
        let events = cache.catch_up();
        if let Some(device) = cache.devices.get(&dev) {
            return device
                .holder_answer(metadata)
                .unwrap_or_else(|| parent_is_bulk_eligible(path));
        }
        events
    };
    // Probed outside the lock: a `statfs` on a slow mount mustn't stall every
    // other re-stat.
    let Some(device) = probe(path, metadata) else {
        return parent_is_bulk_eligible(path);
    };
    {
        let mut cache = cache.lock_ignore_poison();
        // Stored only if no filesystem event landed during the probe.
        if cache.catch_up() == events {
            cache.devices.insert(dev, device);
        }
    }
    device
        .holder_answer(metadata)
        .unwrap_or_else(|| parent_is_bulk_eligible(path))
}

/// Reads the device `metadata` is on: one `statfs` and one `stat` of its mount
/// root, which also proves the `statfs` answered for that device. `None` if
/// either fails or the device changed underneath.
fn probe(path: &Path, metadata: &fs::Metadata) -> Option<Device> {
    // `statfs` follows a final link, and a link lives in its folder's
    // filesystem, so a link asks its folder.
    let on_device = if metadata.file_type().is_symlink() {
        path.parent().filter(|parent| !parent.as_os_str().is_empty())?
    } else {
        path
    };
    let c_path = CString::new(on_device.as_os_str().as_bytes()).ok()?;
    #[cfg(test)]
    super::STATFS_CALLS.with(|calls| calls.set(calls.get() + 1));
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `c_path` is NUL-terminated and outlives the call, and `stat` is a
    // correctly typed out-buffer the kernel fills on success.
    if unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };
    // SAFETY: the kernel NUL-terminates `f_mntonname` within its fixed array.
    let mount_root = unsafe { CStr::from_ptr(stat.f_mntonname.as_ptr()) };
    let root = fs::metadata(Path::new(OsStr::from_bytes(mount_root.to_bytes()))).ok()?;
    (root.dev() == metadata.dev()).then_some(Device {
        eligible: bulk_eligible(&stat),
        root_ino: root.ino(),
    })
}

/// The filesystem events the watcher has counted, or `None` without a
/// watcher, so a test can watch a mount reach it.
#[cfg(test)]
pub(crate) fn filesystem_events() -> Option<u64> {
    cache()?;
    Some(FILESYSTEM_EVENTS.load(Ordering::Acquire))
}
