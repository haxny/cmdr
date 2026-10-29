//! Post-hoc bundle-size cap, plus [`NewestFirstFit`], the measurement both pipelines use to
//! decide how much of a log file fits. Trims log content from the head of each file (line
//! by line) instead of dropping whole entries, so a tight cap still ships the most recent
//! context.
//!
//! Used by Flow B (where the legacy builder doesn't enforce a cap inline) and as a
//! defense-in-depth pass on Flow A in case a future manifest grows large enough to
//! exceed the cap on its own.

use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use zip::DateTime as ZipDateTime;
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

/// Always preserve at least this many lines of the most recent file, even if the cap
/// would otherwise force it down to nothing. [`cap_bundle_to_mb`] may exceed the cap by
/// up to ~10% to honor this. Better to ship 1.1 MB of useful context than 0.
pub(super) const MIN_TAIL_LINES_OF_NEWEST_FILE: usize = 50;

/// Room left per zip entry for its local header and central-directory record.
pub(super) const ENTRY_OVERHEAD_BYTES: usize = 256;

/// How much raw text [`NewestFirstFit`] deflates between measurements. Each measurement is a
/// sync flush (a few bytes of output), and the answer is only as fine as this step: about
/// 1 KB of compressed output on real logs.
const MEASURE_STEP_BYTES: usize = 16 * 1024;

/// `Cursor<Vec<u8>>` adapter that increments an `AtomicU64` by `buf.len()` on every
/// `Write::write` call, so a caller can read how much a `ZipWriter` has emitted while the
/// writer owns the cursor.
///
/// The count is bytes the `zip` crate emitted: compressed payload plus headers, up to the
/// deflater's last flush. Flush the writer before reading it when the answer must be exact.
pub(super) struct CountingCursor {
    inner: std::io::Cursor<Vec<u8>>,
    counter: Arc<AtomicU64>,
}

impl CountingCursor {
    pub(super) fn new(counter: Arc<AtomicU64>) -> Self {
        Self {
            inner: std::io::Cursor::new(Vec::new()),
            counter,
        }
    }

    pub(super) fn into_inner(self) -> Vec<u8> {
        self.inner.into_inner()
    }
}

impl Write for CountingCursor {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for CountingCursor {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Measures how many of a file's NEWEST lines fit in a compressed-byte budget, by deflating
/// them into a scratch entry newest-first, at the real entry's compression level.
///
/// ❗ The cap is on compressed bytes, so the fit must be measured in compressed bytes too.
/// Budgeting raw text against it kept about a seventeenth of what fits on real log text (a
/// 1 MB cap shipped 54 KB), and writing a file oldest-first until the cap tripped dropped its
/// newest lines, the ones the report is about (cmdr-reports#84).
///
/// Offer lines newest-first and stop once [`offer`](Self::offer) says no; [`fitting`](Self::fitting)
/// then says how many of the offered ones fit, measured at [`MEASURE_STEP_BYTES`] steps.
/// Reverse order deflates within a sync flush or two of forward order, which the callers'
/// headroom absorbs.
pub(super) struct NewestFirstFit {
    writer: ZipWriter<CountingCursor>,
    counter: Arc<AtomicU64>,
    budget: u64,
    offered: usize,
    fitting: usize,
    unmeasured_bytes: usize,
    over: bool,
}

impl NewestFirstFit {
    pub(super) fn new(budget: usize, compression_level: Option<i64>) -> Self {
        let counter = Arc::new(AtomicU64::new(0));
        let mut writer = ZipWriter::new(CountingCursor::new(counter.clone()));
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(compression_level);
        let over = writer.start_file("measure", opts).is_err();
        Self {
            writer,
            counter,
            budget: budget as u64,
            offered: 0,
            fitting: 0,
            unmeasured_bytes: 0,
            over,
        }
    }

    /// Offers the next-older line. Returns `false` once the budget is spent: stop offering.
    pub(super) fn offer(&mut self, line: &[u8]) -> bool {
        if self.over {
            return false;
        }
        if self.writer.write_all(line).is_err() || self.writer.write_all(b"\n").is_err() {
            self.over = true;
            return false;
        }
        self.offered += 1;
        self.unmeasured_bytes += line.len() + 1;
        if self.unmeasured_bytes >= MEASURE_STEP_BYTES {
            return self.measure();
        }
        true
    }

    fn measure(&mut self) -> bool {
        self.unmeasured_bytes = 0;
        if self.writer.flush().is_err() || self.counter.load(Ordering::Relaxed) > self.budget {
            self.over = true;
            return false;
        }
        self.fitting = self.offered;
        true
    }

    /// How many of the offered lines (the newest ones) fit the budget.
    pub(super) fn fitting(mut self) -> usize {
        if !self.over && self.unmeasured_bytes > 0 {
            self.measure();
        }
        self.fitting
    }
}

/// Cap the bundle to `cap_mb` megabytes, **trimming log content from the head**, not
/// dropping whole files.
///
/// Behavior:
/// 1. `manifest.json` is always preserved in full (verbatim, with its original mtime).
/// 2. `logs/*` entries are sorted newest-first by their stored mtime so the most recent context
///    wins the budget race.
/// 3. Each log entry keeps the newest tail of its lines whose COMPRESSED size fits what's left
///    of 90% of the cap ([`NewestFirstFit`]). Older lines get dropped.
/// 4. If a single entry won't fit even partially, we still preserve the last
///    [`MIN_TAIL_LINES_OF_NEWEST_FILE`] lines of the newest file, even if it pushes the output ~10%
///    over the cap. Shipping a 1.1 MB bundle with useful tail beats shipping a 542-byte bundle with
///    only the manifest (which is what the broken pre-fix-6 implementation did (see the bug
///    report).
///
/// If the input zip is already under the cap, returns it untouched.
pub fn cap_bundle_to_mb(zip_bytes: Vec<u8>, cap_mb: usize) -> Vec<u8> {
    cap_bundle_to_bytes(zip_bytes, cap_mb * 1024 * 1024)
}

/// Same as [`cap_bundle_to_mb`], but the cap is expressed in raw bytes.
///
/// `cap_bundle_to_mb` is the prod API and quantizes to whole MB, which is too coarse for
/// unit tests: at MB granularity the smallest interesting cap is 1 MB, which forces every
/// test that engages the cap path to build a multi-megabyte fixture and pay the
/// deflate + decompress + re-deflate cost (~2 s warm, > 8 s under check.sh contention).
/// Exposing the same logic at byte precision lets the tests use ~100 KB fixtures and the
/// cap path runs in tens of milliseconds.
pub(crate) fn cap_bundle_to_bytes(zip_bytes: Vec<u8>, cap_bytes: usize) -> Vec<u8> {
    if zip_bytes.len() <= cap_bytes {
        return zip_bytes;
    }

    let Ok(mut archive) = ZipArchive::new(std::io::Cursor::new(&zip_bytes)) else {
        return zip_bytes;
    };

    // Pull the manifest (preserve verbatim with its mtime).
    let manifest_bytes_and_mtime = read_entry_with_mtime(&mut archive, "manifest.json");

    // Inventory log entries with their mtimes so we can sort newest-first.
    struct LogEntry {
        name: String,
        mtime: ZipDateTime,
        content: Vec<u8>,
    }
    let mut log_entries: Vec<LogEntry> = Vec::new();
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else { continue };
        let Ok(name) = entry.name().map(std::borrow::Cow::into_owned) else {
            continue;
        };
        if !name.starts_with("logs/") {
            continue;
        }
        let mtime = entry.last_modified().unwrap_or_default();
        let mut content = Vec::new();
        if entry.read_to_end(&mut content).is_err() {
            continue;
        }
        log_entries.push(LogEntry { name, mtime, content });
    }
    // Newest first.
    log_entries.sort_by_key(|e| std::cmp::Reverse(e.mtime));

    // Headroom: leave 10% for the central directory plus per-entry overhead. Compressed
    // text is hard to predict from line count alone, but 10% has been reliable in the
    // 30 MB → 1 MB regression test.
    let target = (cap_bytes * 9) / 10;

    let counter = Arc::new(AtomicU64::new(0));
    let finish_result = {
        let mut writer = ZipWriter::new(CountingCursor::new(counter.clone()));

        // 1. Manifest, verbatim.
        if let Some((bytes, mtime)) = &manifest_bytes_and_mtime {
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(*mtime);
            if writer.start_file("manifest.json", opts).is_err() || writer.write_all(bytes).is_err() {
                // Manifest write failed; bail to the original.
                return zip_bytes;
            }
        }

        // 2. Logs, newest-first. Each file keeps the newest tail that fits what's left of
        // the budget, measured in COMPRESSED bytes (`NewestFirstFit`): the cap is on the
        // compressed bundle, and a raw-byte tally kept a small fraction of what fits.
        for (i, entry) in log_entries.iter().enumerate() {
            let lines: Vec<&[u8]> = split_into_lines(&entry.content);
            // Flushed, so the count covers everything written so far.
            if writer.flush().is_err() {
                break;
            }
            let used = counter.load(Ordering::Relaxed) as usize;
            let remaining_budget = target.saturating_sub(used + ENTRY_OVERHEAD_BYTES);

            let mut fit = NewestFirstFit::new(remaining_budget, None);
            for line in lines.iter().rev() {
                if !fit.offer(line) {
                    break;
                }
            }
            let mut keep = fit.fitting();
            // Floor: the newest file ships at least N lines, even if that marginally exceeds
            // the cap (see the doc comment).
            if i == 0 {
                keep = keep.max(MIN_TAIL_LINES_OF_NEWEST_FILE.min(lines.len()));
            }
            if keep == 0 {
                // Older files are older still, and the budget is spent.
                break;
            }
            let kept_lines = take_tail(&lines, keep);

            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(entry.mtime);
            if writer.start_file(&entry.name, opts).is_err() {
                continue;
            }
            for line in &kept_lines {
                if writer.write_all(line).is_err() || writer.write_all(b"\n").is_err() {
                    break;
                }
            }
        }

        writer.finish()
    };

    match finish_result {
        Ok(cursor) => {
            let out_buf = cursor.into_inner();
            if out_buf.is_empty() { zip_bytes } else { out_buf }
        }
        Err(_) => zip_bytes,
    }
}

/// Split a log file's content into lines (without trailing newlines) so we can pack
/// them tail-first into the capped zip. Empty trailing slice is dropped; we add the
/// `\n` separator on write-out anyway.
fn split_into_lines(content: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = content.split(|b| *b == b'\n').collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

/// Take the last `n` lines (or all of them if there are fewer).
fn take_tail<'a>(lines: &[&'a [u8]], n: usize) -> Vec<&'a [u8]> {
    let start = lines.len().saturating_sub(n);
    lines[start..].to_vec()
}

/// Reads an entry's bytes plus its stored mtime. `None` if the entry doesn't exist or
/// can't be read.
fn read_entry_with_mtime<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<(Vec<u8>, ZipDateTime)> {
    let mut entry = archive.by_name(name).ok()?;
    let mtime = entry.last_modified().unwrap_or_default();
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some((bytes, mtime))
}
