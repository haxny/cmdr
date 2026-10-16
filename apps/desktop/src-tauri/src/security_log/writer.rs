//! The one writer: a background thread appending lines to `security-events.jsonl`, rotated at
//! [`MAX_BYTES`] into one `.1` file. `record` hands it a line and never waits; a write that
//! doesn't land logs a warning and drops that one line, so recording can never block or fail
//! the action it records.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use super::types::LogLine;
use crate::logging::rotating_file::RotatingFile;

/// The file's name inside the app data dir.
pub const FILE_NAME: &str = "security-events.jsonl";

/// The live file rotates past this. At a handful of events a day, two files hold years.
pub const MAX_BYTES: u64 = 1024 * 1024;

/// Rotated files kept beside the live one, so the log never grows past two files.
const ROTATED_KEEP: usize = 1;

const LOG_TARGET: &str = "security_log";

pub fn file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

/// The file itself, written synchronously. The thread below wraps it; tests drive it directly.
pub struct LogFile {
    file: RotatingFile,
    /// Asked before every write. In production: "no data wipe yet", so a line recorded during
    /// quit can't recreate the file after "Delete all Cmdr data".
    may_write: Box<dyn Fn() -> bool + Send>,
}

impl LogFile {
    pub fn new(path: PathBuf, max_bytes: u64, may_write: Box<dyn Fn() -> bool + Send>) -> Self {
        Self {
            file: RotatingFile::new(path, ROTATED_KEEP, max_bytes),
            may_write,
        }
    }

    /// Appends one line. `Ok(false)` when the gate said no and nothing was written.
    pub fn append(&mut self, line: &LogLine) -> io::Result<bool> {
        if !(self.may_write)() {
            return Ok(false);
        }
        let mut bytes = serde_json::to_vec(line).map_err(io::Error::other)?;
        bytes.push(b'\n');
        // One write, then one flush: `RotatingFile` rotates on flush, so a line is never split.
        self.file.write_all(&bytes)?;
        self.file.flush()?;
        Ok(true)
    }
}

enum Message {
    Line(LogLine),
    Flush(mpsc::Sender<()>),
}

/// The handle `record` sends through. Cheap to clone; the thread ends when the last one drops.
#[derive(Clone)]
pub struct Writer {
    tx: mpsc::Sender<Message>,
}

impl Writer {
    pub fn spawn(mut file: LogFile) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel::<Message>();
        std::thread::Builder::new()
            .name("security-log-writer".into())
            .spawn(move || {
                for message in rx {
                    match message {
                        Message::Line(line) => {
                            if let Err(e) = file.append(&line) {
                                log::warn!(target: LOG_TARGET, "Couldn't write a security event, dropping it: {e}");
                            }
                        }
                        Message::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })?;
        Ok(Self { tx })
    }

    /// Queues `line`. Never blocks: the channel is unbounded, and at this event rate it stays
    /// near empty.
    pub fn send(&self, line: LogLine) {
        if self.tx.send(Message::Line(line)).is_err() {
            log::warn!(target: LOG_TARGET, "The security log writer is gone, dropping an event");
        }
    }

    /// Waits up to `timeout` for every queued line to be written. Returns whether it got there.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (done_tx, done_rx) = mpsc::channel();
        self.tx.send(Message::Flush(done_tx)).is_ok() && done_rx.recv_timeout(timeout).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::security_log::types::{Initiator, SecurityEvent};
    use crate::test_support::TestDir;

    fn line(by: Initiator) -> LogLine {
        LogLine::now(SecurityEvent::McpServerTurnedOn { by })
    }

    fn always() -> Box<dyn Fn() -> bool + Send> {
        Box::new(|| true)
    }

    fn lines_in(path: &Path) -> Vec<LogLine> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).expect("every line is one JSON event"))
            .collect()
    }

    #[test]
    fn appends_one_json_line_per_event() {
        let dir = TestDir::new("security-log-append");
        let path = file_path(&dir);
        let mut file = LogFile::new(path.clone(), MAX_BYTES, always());
        let first = line(Initiator::Launch);
        let second = line(Initiator::User);
        assert!(file.append(&first).expect("writes"));
        assert!(file.append(&second).expect("writes"));
        assert_eq!(lines_in(&path), vec![first, second]);
    }

    #[test]
    fn rotates_at_the_cap_and_keeps_one_rotated_file() {
        let dir = TestDir::new("security-log-rotate");
        let path = file_path(&dir);
        // Each line is ~100 bytes, so a 150-byte cap rotates after every second line.
        let mut file = LogFile::new(path.clone(), 150, always());
        for _ in 0..7 {
            file.append(&line(Initiator::User)).expect("writes");
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .expect("list")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        let mut names = names;
        names.sort();
        assert_eq!(names, vec![FILE_NAME.to_string(), format!("{FILE_NAME}.1")]);
        assert_eq!(lines_in(&path).len(), 1);
        assert_eq!(lines_in(&dir.join(format!("{FILE_NAME}.1"))).len(), 2);
    }

    #[test]
    fn a_missing_data_dir_is_an_error_not_a_panic_and_the_next_line_lands_once_it_exists() {
        let dir = TestDir::new("security-log-missing-dir");
        let data_dir = dir.join("not-yet");
        let path = file_path(&data_dir);
        let mut file = LogFile::new(path.clone(), MAX_BYTES, always());
        assert!(file.append(&line(Initiator::User)).is_err());
        assert!(!data_dir.exists(), "the writer never creates the data dir");

        std::fs::create_dir(&data_dir).expect("create");
        assert!(file.append(&line(Initiator::Launch)).expect("writes"));
        assert_eq!(lines_in(&path).len(), 1);
    }

    #[test]
    fn writes_nothing_once_the_gate_closes() {
        let dir = TestDir::new("security-log-gate");
        let path = file_path(&dir);
        let open = Arc::new(AtomicBool::new(true));
        let gate = Arc::clone(&open);
        let mut file = LogFile::new(path.clone(), MAX_BYTES, Box::new(move || gate.load(Ordering::SeqCst)));
        file.append(&line(Initiator::User)).expect("writes");
        std::fs::remove_file(&path).expect("the wipe removes it");

        open.store(false, Ordering::SeqCst);
        assert!(!file.append(&line(Initiator::User)).expect("no error"));
        assert!(!path.exists(), "a line after a wipe doesn't recreate the file");
    }

    #[test]
    fn the_thread_writes_queued_lines_and_flush_waits_for_them() {
        let dir = TestDir::new("security-log-thread");
        let path = file_path(&dir);
        let writer = Writer::spawn(LogFile::new(path.clone(), MAX_BYTES, always())).expect("spawn");
        writer.send(line(Initiator::Launch));
        writer.send(line(Initiator::User));
        assert!(writer.flush(Duration::from_secs(5)));
        assert_eq!(lines_in(&path).len(), 2);
    }

    #[test]
    fn a_write_error_drops_the_line_and_keeps_the_thread_alive() {
        let dir = TestDir::new("security-log-thread-error");
        let data_dir = dir.join("gone");
        let path = file_path(&data_dir);
        let writer = Writer::spawn(LogFile::new(path.clone(), MAX_BYTES, always())).expect("spawn");
        writer.send(line(Initiator::User));
        assert!(
            writer.flush(Duration::from_secs(5)),
            "the thread survived the failed write"
        );

        std::fs::create_dir(&data_dir).expect("create");
        writer.send(line(Initiator::Launch));
        assert!(writer.flush(Duration::from_secs(5)));
        assert_eq!(lines_in(&path).len(), 1);
    }
}
