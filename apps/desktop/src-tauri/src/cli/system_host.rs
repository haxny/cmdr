//! The real machine behind [`CommandHost`]: the filesystem, `Info.plist` reads, and the
//! administrator prompt.
//!
//! The prompt is a spawned `osascript` running `do shell script … with administrator
//! privileges`, the same thing VS Code's "Install 'code' command in PATH" does. ❌ Not an
//! in-process `NSAppleScript`: that one must run on the main thread, which would freeze the whole
//! app for as long as the password dialog is up.

use std::io;
use std::path::Path;
use std::time::Duration;

use super::install::{AdminOutcome, CommandHost, PathEntry};

/// How long the administrator prompt may stay up before we give up and stop `osascript`, which
/// takes the dialog down with it.
const ADMIN_PROMPT_TIMEOUT: Duration = Duration::from_secs(300);

/// AppleScript's `userCanceledErr`: the user pressed Cancel in the password dialog.
const USER_CANCELED: i32 = -128;

/// The shell command and the prompt arrive as `argv`, never spliced into the script text, so no
/// character in a path can inject AppleScript. The reply is a number we print ourselves: `0`, or
/// the AppleScript error number, so nothing has to read an error message.
const ADMIN_SCRIPT: &str = r#"on run argv
    try
        do shell script (item 1 of argv) with prompt (item 2 of argv) with administrator privileges
        return "0"
    on error number errorNumber
        return errorNumber as text
    end try
end run"#;

pub(crate) struct SystemHost;

impl CommandHost for SystemHost {
    fn entry(&self, path: &Path) -> PathEntry {
        match std::fs::symlink_metadata(path) {
            Err(_) => PathEntry::Missing,
            Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(path) {
                Ok(target) => PathEntry::Symlink(target),
                Err(_) => PathEntry::Other,
            },
            Ok(_) => PathEntry::Other,
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn bundle_id(&self, bundle: &Path) -> Option<String> {
        let info = plist::Value::from_file(bundle.join("Contents/Info.plist")).ok()?;
        Some(
            info.as_dictionary()?
                .get("CFBundleIdentifier")?
                .as_string()?
                .to_string(),
        )
    }

    fn symlink(&self, target: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    fn remove_link(&self, link: &Path) -> io::Result<()> {
        std::fs::remove_file(link)
    }

    fn run_as_admin(&self, shell: &str, prompt: &str) -> AdminOutcome {
        let mut command = tokio::process::Command::new("/usr/bin/osascript");
        command.arg("-e").arg(ADMIN_SCRIPT).arg(shell).arg(prompt);
        // Runs on a blocking-pool thread (the IPC command's `spawn_blocking`), so blocking on the
        // runtime here is allowed, and it buys `output_within`'s kill-on-timeout.
        let output = tauri::async_runtime::block_on(crate::subprocess::output_within(
            "osascript admin prompt",
            ADMIN_PROMPT_TIMEOUT,
            &mut command,
        ));
        match output {
            Ok(output) => parse_admin_reply(&String::from_utf8_lossy(&output.stdout)),
            Err(crate::subprocess::SubprocessError::Spawn(err)) => AdminOutcome::Failed(err.raw_os_error()),
            Err(crate::subprocess::SubprocessError::TimedOut { .. }) => AdminOutcome::Failed(None),
        }
    }
}

/// Read the number [`ADMIN_SCRIPT`] prints.
fn parse_admin_reply(stdout: &str) -> AdminOutcome {
    match stdout.trim().parse::<i32>() {
        Ok(0) => AdminOutcome::Done,
        Ok(USER_CANCELED) => AdminOutcome::Canceled,
        Ok(code) => AdminOutcome::Failed(Some(code)),
        Err(_) => AdminOutcome::Failed(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    #[test]
    fn the_admin_reply_is_read_as_a_number() {
        assert_eq!(parse_admin_reply("0\n"), AdminOutcome::Done);
        assert_eq!(parse_admin_reply("-128\n"), AdminOutcome::Canceled);
        // A shell step that exits 1 (say `ln` finding something already there) comes back as 1.
        assert_eq!(parse_admin_reply("1\n"), AdminOutcome::Failed(Some(1)));
        assert_eq!(parse_admin_reply(""), AdminOutcome::Failed(None));
    }

    #[test]
    fn entries_are_read_without_following_links() {
        let dir = TestDir::new("cli_host_entries");
        std::fs::write(dir.join("file"), b"x").expect("write");
        std::os::unix::fs::symlink("/nowhere/at/all", dir.join("dangling")).expect("symlink");
        assert_eq!(SystemHost.entry(&dir.join("file")), PathEntry::Other);
        assert_eq!(SystemHost.entry(&dir.join("missing")), PathEntry::Missing);
        // A dangling link is still a link: a moved Cmdr leaves exactly this behind.
        assert_eq!(
            SystemHost.entry(&dir.join("dangling")),
            PathEntry::Symlink("/nowhere/at/all".into())
        );
    }

    #[test]
    fn a_link_never_replaces_what_is_already_there() {
        let dir = TestDir::new("cli_host_no_clobber");
        std::fs::write(dir.join("cmdr"), b"someone else's").expect("write");
        let err = SystemHost
            .symlink(Path::new("/x"), &dir.join("cmdr"))
            .expect_err("must not replace");
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(dir.join("cmdr")).expect("read"), b"someone else's");
    }

    #[test]
    fn a_missing_folder_reports_not_found_so_the_admin_path_can_create_it() {
        let dir = TestDir::new("cli_host_no_dir");
        let err = SystemHost
            .symlink(Path::new("/x"), &dir.join("bin/cmdr"))
            .expect_err("no folder");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn the_bundle_id_comes_from_the_info_plist() {
        let dir = TestDir::new("cli_host_bundle_id");
        let bundle = dir.join("Cmdr.app");
        std::fs::create_dir_all(bundle.join("Contents")).expect("mkdir");
        std::fs::write(
            bundle.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.veszelovszki.cmdr</string></dict></plist>"#,
        )
        .expect("write");
        assert_eq!(SystemHost.bundle_id(&bundle).as_deref(), Some("com.veszelovszki.cmdr"));
        assert_eq!(SystemHost.bundle_id(&dir.join("Gone.app")), None);
    }
}
