package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runProbeUnwrapOn writes the supplied files under
// `apps/desktop/src-tauri/src/file_system/` (rel paths keep their subdirs) and
// runs the check.
func runProbeUnwrapOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	fsDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src", "file_system")
	for rel, body := range files {
		full := filepath.Join(fsDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunProbeUnwrapJustified(&CheckContext{RootDir: root})
}

func TestProbeUnwrap_FlagsASwallowedProbeInProduction(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"conflict.rs": `
async fn resolve(volume: &Arc<dyn Volume>, p: &Path) -> bool {
    volume.is_directory(p).await.unwrap_or(false)
}
`,
	})
	if err == nil {
		t.Fatal("expected a violation, got success")
	}
	if !strings.Contains(err.Error(), "conflict.rs:3") {
		t.Errorf("expected conflict.rs:3, got: %s", err.Error())
	}
}

func TestProbeUnwrap_HonorsTheDirectiveAbove(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"rename.rs": `
async fn label(volume: &Arc<dyn Volume>, p: &Path) -> bool {
    // allowed-probe-unwrap: labels an undo row, reaches no destructive branch
    volume.is_directory(p).await.unwrap_or(false)
}
`,
	})
	if err != nil {
		t.Fatalf("directive should excuse the site, got: %s", err.Error())
	}
}

func TestProbeUnwrap_HonorsATrailingDirective(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"rename.rs": `
async fn label(volume: &Arc<dyn Volume>, p: &Path) -> bool {
    volume.is_directory(p).await.unwrap_or(false) // allowed-probe-unwrap: journal only
}
`,
	})
	if err != nil {
		t.Fatalf("trailing directive should excuse the site, got: %s", err.Error())
	}
}

func TestProbeUnwrap_FailsOnAnOrphanDirective(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"clean.rs": `
// allowed-probe-unwrap: nothing here to excuse
pub fn nothing() {}
`,
	})
	if err == nil {
		t.Fatal("expected an orphan-directive failure, got success")
	}
	if !strings.Contains(err.Error(), "unused") {
		t.Errorf("expected an unused-directive report, got: %s", err.Error())
	}
}

// A dedicated test file reads a final state in an assertion; it drives no
// branch, so it's out of jurisdiction.
func TestProbeUnwrap_IgnoresDedicatedTestFiles(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"merge_tests.rs": `
#[tokio::test]
async fn landed() {
    assert!(dest.is_directory(Path::new("/x")).await.unwrap_or(false));
}
`,
		"volume/backends/smb_semantics_test.rs": `
async fn t() {
    assert!(vol.is_directory(Path::new("/y")).await.unwrap_or(false));
}
`,
		"transfer/volume/strategy_test_support.rs": `
async fn lying_delete(inner: &Arc<dyn Volume>, p: &Path) {
    if inner.is_directory(p).await.unwrap_or(false) { }
}
`,
		// One production file, so the governed tree isn't empty.
		"mod.rs": "pub mod volume;\n",
	})
	if err != nil {
		t.Fatalf("test files are out of jurisdiction, got: %s", err.Error())
	}
}

// The carve-out the check needs `advanceTestModRegion` in INVERTED polarity for:
// a test double living inside a production file's inline `#[cfg(test)] mod`.
func TestProbeUnwrap_IgnoresAnInlineTestModInAProductionFile(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"conflict.rs": `
pub fn production() -> bool { true }

#[cfg(test)]
mod tests {
    impl Volume for RecursiveDeleteVolume {
        fn delete(&self, path: &Path) {
            if self.inner.is_directory(path).await.unwrap_or(false) { }
        }
    }
}
`,
	})
	if err != nil {
		t.Fatalf("an inline test mod is out of jurisdiction, got: %s", err.Error())
	}
}

// The production line before an inline test mod is still in jurisdiction: the
// carve-out must not swallow the whole file.
func TestProbeUnwrap_StillFlagsProductionAboveAnInlineTestMod(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"conflict.rs": `
async fn production(volume: &Arc<dyn Volume>, p: &Path) -> bool {
    volume.is_directory(p).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    fn nothing() {}
}
`,
	})
	if err == nil {
		t.Fatal("expected the production site to be flagged, got success")
	}
	if !strings.Contains(err.Error(), "conflict.rs:3") {
		t.Errorf("expected conflict.rs:3, got: %s", err.Error())
	}
}

// Out of scope on purpose: the rule is about `is_directory`, whose wrong answer
// picks a branch that deletes. Widening to `exists` would double the finding set
// with mostly-truthful sites.
func TestProbeUnwrap_IgnoresOtherProbes(t *testing.T) {
	_, err := runProbeUnwrapOn(t, map[string]string{
		"probe.rs": `
async fn f(volume: &Arc<dyn Volume>, p: &Path) -> bool {
    volume.exists(p).await.unwrap_or(false)
}
`,
	})
	if err != nil {
		t.Fatalf("only `is_directory` is in scope, got: %s", err.Error())
	}
}

// A tree outside `file_system/` has no `Volume::is_directory` to swallow.
func TestProbeUnwrap_IgnoresTreesOutsideFileSystem(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	// One production file under `file_system/`, so the governed tree isn't empty.
	writeFixtureFiles(t, filepath.Join(root, "apps", "desktop", "src-tauri", "src"), map[string]string{
		"file_system/mod.rs": "pub mod volume;\n",
	})
	other := filepath.Join(root, "apps", "desktop", "src-tauri", "src", "mtp")
	if err := os.MkdirAll(other, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	body := "async fn f() { v.is_directory(p).await.unwrap_or(false); }\n"
	if err := os.WriteFile(filepath.Join(other, "conn.rs"), []byte(body), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	if _, err := RunProbeUnwrapJustified(&CheckContext{RootDir: root}); err != nil {
		t.Fatalf("outside `file_system/` is out of scope, got: %s", err.Error())
	}
}

// All of `cmdr-local` is in scope: the local backend answers `is_directory` itself,
// and has no `file_system/` subdir to narrow to.
func TestProbeUnwrap_CoversTheWholeCmdrLocalTree(t *testing.T) {
	root := t.TempDir()
	seedCmdrLocalFixtureWorkspace(t, root)
	writeFixtureFiles(t, filepath.Join(root, "crates", "cmdr-local", "src"), map[string]string{
		"local_posix/scan.rs": "async fn f() { v.is_directory(p).await.unwrap_or(false); }\n",
	})
	_, err := RunProbeUnwrapJustified(&CheckContext{RootDir: root})
	if err == nil {
		t.Fatal("expected a violation in cmdr-local, got success")
	}
	if !strings.Contains(err.Error(), "scan.rs:1") {
		t.Errorf("expected scan.rs:1, got: %s", err.Error())
	}
}

// A governed tree that scans nothing is a failure, never a pass: it's what a tree
// moving out from under the check looks like.
func TestProbeUnwrap_FailsWhenAGovernedTreeScansNothing(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	if err := os.MkdirAll(filepath.Join(root, "apps", "desktop", "src-tauri", "src"), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	_, err := RunProbeUnwrapJustified(&CheckContext{RootDir: root})
	if err == nil {
		t.Fatal("expected an empty governed tree to fail, got success")
	}
	if !strings.Contains(err.Error(), "apps/desktop/src-tauri/src") {
		t.Errorf("expected the empty tree named, got: %s", err.Error())
	}
}
