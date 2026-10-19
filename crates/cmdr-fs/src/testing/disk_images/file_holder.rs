//! [`FileHolder`]: a file held open from a child process, so its volume refuses to
//! unmount.

use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// A child process holding a file open, so the volume under it can't unmount. It
/// descends from the test process: identify it by [`Self::pid`], never by kind.
#[derive(Debug)]
pub struct FileHolder {
    child: Child,
}

impl FileHolder {
    /// Starts `/bin/sleep` with `path` open as its stdin.
    pub fn hold(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let child = Command::new("/bin/sleep")
            .arg("600")
            .stdin(Stdio::from(file))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self { child })
    }

    /// The holder's PID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for FileHolder {
    fn drop(&mut self) {
        // allowed-discarded-outcome: a holder that already exited has nothing left to kill
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
