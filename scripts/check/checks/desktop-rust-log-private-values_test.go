package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// logPrivateFixtureRoot builds a fixture repo with the given files under the app
// tree, plus an optional allowlist, and returns the root.
func logPrivateFixtureRoot(t *testing.T, files map[string]string, allowlist map[string]logPrivateAllowance) string {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	for rel, body := range files {
		mustWrite(t, filepath.Join(srcDir, rel), body)
	}
	if allowlist != nil {
		path := filepath.Join(root, logPrivateAllowlistRelPath)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := writeJSONAllowlist(path, logPrivateAllowlist{Files: allowlist}); err != nil {
			t.Fatalf("seed allowlist: %v", err)
		}
	}
	return root
}

func runLogPrivateOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	return RunLogPrivateValues(&CheckContext{RootDir: logPrivateFixtureRoot(t, files, nil)})
}

// expectLogPrivateFlags runs the check over one file and asserts it fails naming
// every given `file.rs:line`, and only as many sites as listed.
func expectLogPrivateFlags(t *testing.T, body string, lines ...string) {
	t.Helper()
	_, err := runLogPrivateOn(t, map[string]string{"site.rs": body})
	if err == nil {
		t.Fatalf("expected unmarked values at %v, got success", lines)
	}
	for _, l := range lines {
		if !strings.Contains(err.Error(), "site.rs:"+l+" ") {
			t.Errorf("expected a finding at site.rs:%s, got:\n%s", l, err.Error())
		}
	}
	if got := strings.Count(err.Error(), "      apps/desktop/src-tauri/src/site.rs:"); got != len(lines) {
		t.Errorf("expected %d findings, got %d:\n%s", len(lines), got, err.Error())
	}
}

func expectLogPrivateClean(t *testing.T, files map[string]string) {
	t.Helper()
	res, err := runLogPrivateOn(t, files)
	if err != nil {
		t.Fatalf("expected success, got:\n%v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLogPrivateValues_FlagsDisplayAndLossyConversions(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(p: &Path, q: &OsStr) {
    log::info!("Listing {}", p.display());
    log::debug!("Got {:?}", q.to_string_lossy());
}
`, "3", "4")
}

func TestLogPrivateValues_FlagsArgumentsNamedLikeAPathOrIdentity(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(src_path: &Path, entry: &Entry, volume_name: &str, hosts: &[String]) {
    log::debug!("copy {:?}", src_path);
    log::debug!("entry {}", entry.name);
    log::debug!("entry {:?}", entry.path());
    log::warn!("volume {} gone", volume_name);
    log::info!("hosts {:?}", hosts);
    log::info!("name {}", file_name_str);
}
`, "3", "4", "5", "6", "7", "8")
}

func TestLogPrivateValues_FlagsInlineCaptures(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(path: &Path, share: &str, count: usize) {
    log::debug!("listed {path:?} on {share} ({count} entries)");
    log::debug!("state lives in {STATE_FILE_PATH}");
}
`, "3", "3")
}

func TestLogPrivateValues_ReportsTheArgumentLineOfAMultiLineMacro(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(dir: &Path, n: usize) {
    log::info!(
        target: "listing",
        "listed {} ({} entries)",
        dir.display(),
        n,
    );
}
`, "6")
}

func TestLogPrivateValues_CoversEveryLogMacroSpelling(t *testing.T) {
	expectLogPrivateFlags(t, `
use log::{debug, error};
fn f(path: &Path, level: log::Level) {
    debug!("a {}", path.display());
    error!("b {}", path.display());
    trace!("c {}", path.display());
    log::log!(level, "d {}", path.display());
    log_error!("e {}", path.display());
    crate::log_error!("f {}", path.display());
    log::warn!["g {}", path.display()];
}
`, "4", "5", "6", "7", "8", "9", "10")
}

func TestLogPrivateValues_AcceptsWrappedValues(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{"site.rs": `
use cmdr_fs::log_private::LogPrivate;
fn f(path: &Path, err: &io::Error, message: &str, dir: Option<&Path>) {
    log::debug!("listed {:?}", LogPrivate::dir_path(path));
    log::debug!("listed {}", &LogPrivate::file_path(&path));
    log::debug!("listed {:?}", cmdr_fs::log_private::LogPrivate::path(&path.display().to_string()));
    log::warn!("detail={:?}", LogDetail(&err.to_string()).marking(&[LogPrivate::path(path)]));
    log::warn!("detail={:?}", cmdr_fs::LogDetail(message));
    log::warn!("note {:?}", LoggedText(message));
    log::debug!("maybe {:?}", dir.map(LogPrivate::dir_path));
    log::debug!("named {p}", p = LogPrivate::path(path));
}
`})
}

func TestLogPrivateValues_LeavesNonIdentityValuesAlone(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{"site.rs": `
fn f(paths: &[PathBuf], path: &Path, entry: &Entry, elapsed: Duration, file_count: usize, volume_id: &str) {
    log::debug!("{} paths in {:?}", paths.len(), elapsed);
    log::debug!("exists={} dir={}", path.exists(), path.is_dir());
    log::debug!("name is {} long", entry.name.len());
    log::debug!("ext {:?}", path.extension());
    log::debug!("{file_count} files on {volume_id}");
    log::debug!("{{path}} is a literal brace pair, {} too", file_count);
    log::debug!("a (paren, comma) and \"quote\" {}", file_count);
    log::debug!(r#"raw "(path)" {}"#, file_count);
    log::debug!("{:?}", paths.iter().collect::<HashMap<_, _>>().len());
    log::debug!("{}", Path::new("/tmp").exists());
    log::debug!("{}", count_of(file_count));
    println!("{}", path.display());
    let s = format!("{}", path.display());
    // log::debug!("{}", path.display());
    /* log::debug!("{}", path.display()); */
}
`})
}

func TestLogPrivateValues_SeesThroughStringsWithParensAndChars(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(path: &Path, c: char) {
    log::debug!("open ( paren \" ) {} {}", c == '(', path.display());
    log::debug!("next {}", path.display());
}
`, "3", "4")
}

func TestLogPrivateValues_JudgesANestedFormatOnce(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(path: &Path) {
    log::debug!("{}", format!("at {}", path.display()));
}
`, "3")
}

func TestLogPrivateValues_SkipsTestCode(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{
		"site_test.rs":     `fn t(p: &Path) { log::debug!("{}", p.display()); }`,
		"tests/fixture.rs": `fn t(p: &Path) { log::debug!("{}", p.display()); }`,
		"site.rs": `
#[cfg(test)]
mod split_tests;

fn real() {}

#[cfg(test)]
mod tests {
    fn t(p: &Path) {
        log::debug!("{}", p.display());
    }
}
`,
	})
}

func TestLogPrivateValues_AnOutOfLineTestModDoesNotHideTheNextItem(t *testing.T) {
	expectLogPrivateFlags(t, `
#[cfg(test)]
mod tests;

fn real(p: &Path) {
    log::debug!("{}", p.display());
}
`, "6")
}

func TestLogPrivateValues_HonorsTheOptOut(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{"site.rs": `
fn f(name: &str, thread_name: &str) {
    // allowed-log-private: the setting's name, not the user's
    log::debug!("setting {} changed", name);
    log::debug!("thread {}", thread_name); // allowed-log-private: a thread's name
}
`})
}

func TestLogPrivateValues_FlagsAnOrphanedOptOut(t *testing.T) {
	_, err := runLogPrivateOn(t, map[string]string{"site.rs": `
fn f(n: usize) {
    // allowed-log-private: nothing to excuse here
    log::debug!("{} entries", n);
}
`})
	if err == nil || !strings.Contains(err.Error(), "site.rs:3") {
		t.Fatalf("expected the orphaned opt-out at site.rs:3, got: %v", err)
	}
}

func TestLogPrivateValues_FlagsAMarkedValueTurnedIntoAString(t *testing.T) {
	_, err := runLogPrivateOn(t, map[string]string{"site.rs": `
fn f(path: &Path, text: &str) -> (String, String, String) {
    let a = LogPrivate::path(path).to_string();
    let b = format!("{}", LogPrivate::dir_path(path));
    let c = format!("{:?}", LogDetail(text));
    log::debug!("{}", format!("{}", LogPrivate::path(path)));
    let note = format!("detail={:?}", LoggedText(text));
    (a, b, c)
}
`})
	if err == nil {
		t.Fatal("expected marked strings outside a log macro to fail")
	}
	for _, line := range []string{"site.rs:3", "site.rs:4", "site.rs:5"} {
		if !strings.Contains(err.Error(), line) {
			t.Errorf("expected %s, got:\n%s", line, err.Error())
		}
	}
	for _, line := range []string{"site.rs:6", "site.rs:7"} {
		if strings.Contains(err.Error(), line) {
			t.Errorf("did not expect %s (inside a log macro, or re-quoting our own log text), got:\n%s", line, err.Error())
		}
	}
}

func TestLogPrivateValues_AllowlistSuppressesKnownSites(t *testing.T) {
	root := logPrivateFixtureRoot(t, map[string]string{"known.rs": `
fn f(path: &Path) {
    log::debug!("{}", path.display());
    log::debug!("{:?}", path);
}
`}, map[string]logPrivateAllowance{
		"apps/desktop/src-tauri/src/known.rs": {Count: 2, Reason: "predates the convention"},
	})
	res, err := RunLogPrivateValues(&CheckContext{RootDir: root})
	if err != nil {
		t.Fatalf("expected success for allowlisted sites, got: %v", err)
	}
	if res.Code != ResultSuccess || !strings.Contains(res.Message, "2 unmarked values allowlisted") {
		t.Fatalf("expected success naming the allowlisted count, got %v: %s", res.Code, res.Message)
	}
}

func TestLogPrivateValues_AllowlistRatchetsDownAndFailsOnGrowth(t *testing.T) {
	root := logPrivateFixtureRoot(t, map[string]string{
		"shrunk.rs": `fn f(p: &Path) { log::debug!("{}", p.display()); }`,
		"grown.rs": `
fn f(p: &Path) {
    log::debug!("{}", p.display());
    log::debug!("{}", p.display());
}
`,
	}, map[string]logPrivateAllowance{
		"apps/desktop/src-tauri/src/shrunk.rs": {Count: 3, Reason: "kept reason"},
		"apps/desktop/src-tauri/src/grown.rs":  {Count: 1, Reason: "grown reason"},
		"apps/desktop/src-tauri/src/gone.rs":   {Count: 4, Reason: "gone reason"},
	})
	_, err := RunLogPrivateValues(&CheckContext{RootDir: root})
	if err == nil {
		t.Fatal("expected the grown file to fail")
	}
	if !strings.Contains(err.Error(), "grown.rs:4") || !strings.Contains(err.Error(), "(allowlist: 1)") {
		t.Errorf("expected grown.rs's sites listed against its budget, got:\n%s", err.Error())
	}
	if strings.Contains(err.Error(), "shrunk.rs:1") {
		t.Errorf("did not expect shrunk.rs (within budget) listed, got:\n%s", err.Error())
	}
	list := loadLogPrivateAllowlist(root)
	if got := list.Files["apps/desktop/src-tauri/src/shrunk.rs"]; got.Count != 1 || got.Reason != "kept reason" {
		t.Errorf("expected shrunk.rs ratcheted 3 → 1 with its reason kept, got %+v", got)
	}
	if _, ok := list.Files["apps/desktop/src-tauri/src/gone.rs"]; ok {
		t.Error("expected the entry for the deleted file to be dropped")
	}
	if got := list.Files["apps/desktop/src-tauri/src/grown.rs"]; got.Count != 1 {
		t.Errorf("expected grown.rs to keep its budget at 1, got %d", got.Count)
	}
}

func TestLogPrivateValues_CIReportsStalenessWithoutRewriting(t *testing.T) {
	root := logPrivateFixtureRoot(t, map[string]string{
		"shrunk.rs": `fn f(p: &Path) { log::debug!("{}", p.display()); }`,
	}, map[string]logPrivateAllowance{
		"apps/desktop/src-tauri/src/shrunk.rs": {Count: 3, Reason: "kept reason"},
	})
	res, err := RunLogPrivateValues(&CheckContext{RootDir: root, CI: true})
	if err != nil {
		t.Fatalf("expected success, got: %v", err)
	}
	if !strings.Contains(res.Message, "a local run shrink-wraps them") {
		t.Errorf("expected the CI staleness note, got: %s", res.Message)
	}
	if got := loadLogPrivateAllowlist(root).Files["apps/desktop/src-tauri/src/shrunk.rs"]; got.Count != 3 {
		t.Errorf("expected CI to leave the allowlist alone, got %d", got.Count)
	}
}

func TestLogPrivateValues_AnEntryWithoutAReasonFails(t *testing.T) {
	root := logPrivateFixtureRoot(t, map[string]string{
		"known.rs": `fn f(p: &Path) { log::debug!("{}", p.display()); }`,
	}, map[string]logPrivateAllowance{
		"apps/desktop/src-tauri/src/known.rs": {Count: 1},
	})
	_, err := RunLogPrivateValues(&CheckContext{RootDir: root})
	if err == nil || !strings.Contains(err.Error(), "known.rs") || !strings.Contains(err.Error(), "reason") {
		t.Fatalf("expected a reasonless entry to fail, got: %v", err)
	}
}

func TestReadsAsIdentity(t *testing.T) {
	for _, name := range []string{
		"path", "src_path", "dest", "dst", "volume_name", "path_str", "paths", "filename", "hostname",
		"username", "server", "share", "label", "root", "roots", "dir", "directories", "parent_dir",
		"name_lower", "_path", "folder", "source", "host", "user", "file", "subdir", "mount_point",
	} {
		if !readsAsIdentity(name) {
			t.Errorf("expected %q to read as a path or identity", name)
		}
	}
	for _, name := range []string{
		"file_count", "path_len", "profile", "rename", "volume_id", "entries", "user_id", "server_id",
		"count", "elapsed", "nickname", "e", "err", "self", "kind",
		// Counts: a bare plural of a file-system noun is a tally in a log line,
		// and so is a name that leads with a quantity word.
		"files", "dirs", "folders", "kept_files", "total_dirs", "num_paths", "n_files",
		// Flags: a name carrying a predicate word holds a bool.
		"is_dir", "is_directory", "source_is_dir", "in_subdir", "use_concurrent_path", "has_parent",
	} {
		if readsAsIdentity(name) {
			t.Errorf("did not expect %q to read as a path or identity", name)
		}
	}
}
