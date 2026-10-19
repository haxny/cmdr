//! Surviving a read of a memory-mapped file whose disk went away.
//!
//! `gix` maps a repository's pack files, and Apple's ImageIO, QuickLook, and PDFKit map the
//! files they read. When the volume under such a mapping goes away (a pulled USB cable, a forced
//! eject, a network volume dropping), the next read of a page that isn't resident raises SIGBUS,
//! and SIGBUS kills the process. [`recover`] is what a SIGBUS handler calls first: when the fault
//! is a READ of a read-only, file-backed page that belongs to no loaded image, it maps anonymous
//! zeros over that one page, moves [`generation`], and the faulting load re-executes and reads
//! zeros. Every other fault is left alone, for the crash reporter.
//!
//! **Why zeros are sound.** Nothing unwinds and no Rust frame is skipped: the instruction that
//! faulted runs again and sees valid memory. A decoder reading zeros sees corrupt data and says
//! so through its own error path. What a caller must do is distrust any answer computed while
//! [`generation`] moved, which is the caller's job (`crates/cmdr-git/src/fault_guard.rs`), since
//! only it knows what an answer is.
//!
//! **What is never recovered**, each a fault where zeros would be worse than the crash:
//!
//! - A page of a loaded image (our binary, a dylib, a framework, the dyld shared cache): a code
//!   page of zeros executes as garbage. macOS keeps the image ranges in a table filled from
//!   dyld's add-image callback at [`prepare`] time; Linux recognizes an image by the same file
//!   also being mapped executable.
//! - A writable mapping: a write landing in an anonymous page would look committed and never
//!   reach the file, which is silent data loss (SQLite's `-shm` is mapped this way).
//! - An executable or anonymous page, and any address no file-backed region covers.
//!
//! **Async-signal-safe.** [`recover`] allocates nothing, takes no lock, and formats nothing. It
//! makes three kinds of call: atomic loads, `mmap` (a raw syscall), and, on macOS,
//! `mach_vm_region_recurse` (a Mach trap through the thread's MIG reply port, the call crash
//! reporters make from their own handlers) or, on Linux, `open` / `read` / `close` of
//! `/proc/self/maps`, all three on POSIX's async-signal-safe list.
//!
//! The why and the evidence: `DETAILS.md` § "Mapped-file faults".

use std::sync::atomic::{AtomicU64, Ordering};

mod image_ranges;
mod proc_maps;
mod verdict;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "macos"))]
mod real_image;

/// How many pages [`recover`] has mapped zeros over since the process started.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// How many mapped-file pages have been recovered since the process started.
///
/// Read it before and after an operation that reads through a mapping: if it moved, some page
/// read in between may have been zeros, so the answer can't be trusted. The counter is
/// process-wide, because the fault can land on a thread the operation doesn't own (`gix` runs its
/// status walk on threads it spawns itself).
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Readies [`recover`]: reads the page size and, on macOS, starts tracking every loaded image's
/// address range. Idempotent. Call it in normal context, before installing a handler that calls
/// [`recover`]; until it ran, [`recover`] recovers nothing.
pub fn prepare() {
    platform::prepare();
}

/// Called first by a SIGBUS handler. Maps zeros over the faulting page and answers `true` when
/// the fault is a read of a read-only, file-backed page outside every loaded image; then the
/// handler must return, so the load re-executes. Answers `false`, touching nothing, for every
/// other fault, which the handler then treats as the crash it is.
///
/// # Safety
///
/// Call it only from a signal handler for `sig`, with the `siginfo_t` the kernel passed that
/// handler. It's async-signal-safe (see the module docs).
pub unsafe fn recover(sig: libc::c_int, info: *const libc::siginfo_t) -> bool {
    if sig != libc::SIGBUS || info.is_null() {
        return false;
    }
    // SAFETY: the caller passes the kernel's `siginfo_t` for this SIGBUS, which is live for the
    // handler's duration.
    let recovered = unsafe { platform::recover(&*info) };
    if recovered {
        GENERATION.fetch_add(1, Ordering::SeqCst);
    }
    recovered
}

/// Moves [`generation`] as one recovered page would, so a consumer's cell can drive what follows a
/// recovery without a disk image. ❌ Never called outside a test.
#[cfg(any(test, feature = "testing"))]
pub fn simulate_recovery() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Maps one page of anonymous, read-only zeros over `page`.
///
/// `MAP_FIXED` replaces exactly that page of the dead file mapping and nothing else, so its owner
/// can later `munmap` the whole range as it always would.
fn map_zeros_over(page: u64, page_size: u64) -> bool {
    // SAFETY: `page` is page-aligned and lies inside a file-backed, read-only, non-image region
    // the caller just looked up, so replacing it changes only memory whose next read would
    // otherwise kill the process. `mmap` is a raw syscall: async-signal-safe in practice.
    let mapped = unsafe {
        libc::mmap(
            page as *mut libc::c_void,
            page_size as libc::size_t,
            libc::PROT_READ,
            libc::MAP_FIXED | libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    mapped != libc::MAP_FAILED
}

/// A SIGBUS handler for a test process that has no crash reporter: it calls [`recover`] and, for
/// a fault it can't own, restores the default action and returns, so the load re-executes and the
/// process dies of the signal as it would have.
#[cfg(any(test, feature = "testing"))]
pub fn install_test_handler() {
    extern "C" fn on_bus(sig: libc::c_int, info: *mut libc::siginfo_t, _context: *mut libc::c_void) {
        // SAFETY: the kernel hands this handler its own `siginfo_t`.
        if unsafe { recover(sig, info) } {
            return;
        }
        // SAFETY: `signal` is async-signal-safe; restoring the default makes the re-executed load
        // fatal.
        unsafe { libc::signal(sig, libc::SIG_DFL) };
    }
    prepare();
    // SAFETY: `action` starts zeroed, `sa_mask` is emptied, and `sa_sigaction` is a valid
    // `SA_SIGINFO` handler with the matching signature.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_bus as *const () as usize;
        action.sa_flags = libc::SA_SIGINFO;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGBUS, &action, std::ptr::null_mut());
    }
}
