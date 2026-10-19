package checks

import (
	"bufio"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"
)

func writeCorpusFixture(t *testing.T, root string, files map[string]string) {
	t.Helper()
	for rel, body := range files {
		full := filepath.Join(root, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
}

// The corpus is a SNAPSHOT: once a file is read, every later scanner gets the
// same bytes without touching the disk again. Deleting the file after the first
// read is how a test can see that no second read happened.
func TestCorpusReadsEachFileOnce(t *testing.T) {
	root := t.TempDir()
	writeCorpusFixture(t, root, map[string]string{"a.rs": "fn a() {}\n"})
	c := newSourceCorpus()
	path := filepath.Join(root, "a.rs")

	first, err := c.ReadFile(path)
	if err != nil {
		t.Fatalf("first read: %v", err)
	}
	if err := os.Remove(path); err != nil {
		t.Fatalf("remove: %v", err)
	}
	second, err := c.ReadFile(path)
	if err != nil {
		t.Fatalf("second read hit the disk: %v", err)
	}
	if first != second || first != "fn a() {}\n" {
		t.Errorf("got %q then %q", first, second)
	}
}

// A missing file stays an `fs.ErrNotExist`, so a caller's `os.IsNotExist` /
// `errors.Is` branch keeps working through the corpus.
func TestCorpusKeepsNotExistErrors(t *testing.T) {
	c := newSourceCorpus()
	_, err := c.ReadFile(filepath.Join(t.TempDir(), "missing.rs"))
	if !errors.Is(err, fs.ErrNotExist) || !os.IsNotExist(err) {
		t.Errorf("want a not-exist error, got %v", err)
	}
}

// Lines must split exactly as the `bufio.Scanner` loops it replaces did: no
// trailing empty line for a final newline, a trailing `\r` dropped, an empty
// file yielding nothing.
func TestCorpusLinesMatchBufioScanner(t *testing.T) {
	bodies := []string{
		"", "\n", "a", "a\n", "a\nb", "a\nb\n", "a\n\n", "a\r\nb\r\n", "a\r", "\n\nx\n\n", "tail\r\n\r\n",
	}
	root := t.TempDir()
	c := newSourceCorpus()
	for i, body := range bodies {
		path := filepath.Join(root, strings.Repeat("f", i+1))
		writeCorpusFixture(t, root, map[string]string{filepath.Base(path): body})

		var want []string
		scanner := bufio.NewScanner(strings.NewReader(body))
		for scanner.Scan() {
			want = append(want, scanner.Text())
		}
		got, err := c.Lines(path)
		if err != nil {
			t.Fatalf("Lines(%q): %v", body, err)
		}
		if !reflect.DeepEqual(got, want) && !(len(got) == 0 && len(want) == 0) {
			t.Errorf("Lines(%q) = %q, bufio.Scanner gives %q", body, got, want)
		}
	}
}

// WalkDir visits what `filepath.WalkDir` visits, in the same order, honoring
// SkipDir, so a scanner can switch over without its own filtering changing.
func TestCorpusWalkDirMatchesFilepathWalkDir(t *testing.T) {
	root := t.TempDir()
	writeCorpusFixture(t, root, map[string]string{
		"src/lib.rs":                 "",
		"src/a/mod.rs":               "",
		"src/a/b/deep.rs":            "",
		"src/skip/never.rs":          "",
		"src/z.txt":                  "",
		"src/tests/fixture_tests.rs": "",
	})
	visit := func(walk func(string, fs.WalkDirFunc) error, start string) []string {
		var seen []string
		err := walk(start, func(path string, d fs.DirEntry, err error) error {
			if err != nil {
				seen = append(seen, "ERR "+filepath.Base(path))
				return nil
			}
			rel, _ := filepath.Rel(root, path)
			seen = append(seen, filepath.ToSlash(rel)+map[bool]string{true: "/", false: ""}[d.IsDir()])
			if d.IsDir() && d.Name() == "skip" {
				return filepath.SkipDir
			}
			return nil
		})
		if err != nil {
			t.Fatalf("walk: %v", err)
		}
		return seen
	}
	c := newSourceCorpus()
	for _, start := range []string{filepath.Join(root, "src"), filepath.Join(root, "missing"), filepath.Join(root, "src", "lib.rs")} {
		want := visit(filepath.WalkDir, start)
		got := visit(c.WalkDir, start)
		if !reflect.DeepEqual(got, want) {
			t.Errorf("WalkDir(%s):\n got  %q\n want %q", filepath.Base(start), got, want)
		}
		// The second walk comes from the cached listing and must agree too.
		if again := visit(c.WalkDir, start); !reflect.DeepEqual(again, want) {
			t.Errorf("cached WalkDir(%s):\n got  %q\n want %q", filepath.Base(start), again, want)
		}
	}
}

// The doc scanners each asked git for the same listing; the corpus asks once per
// distinct argument list, and a listing that fails stays an error (not a run
// that "listed nothing").
func TestCorpusGitLsFilesRunsOncePerArgs(t *testing.T) {
	root := t.TempDir()
	gitInit(t, root)
	writeCorpusFixture(t, root, map[string]string{"a.md": "", "b.rs": "", "dir/c.md": ""})
	gitRun(t, root, "add", ".")
	c := newSourceCorpus()

	all, err := c.GitLsFiles(root)
	if err != nil {
		t.Fatalf("ls-files: %v", err)
	}
	if want := []string{"a.md", "b.rs", "dir/c.md"}; !reflect.DeepEqual(all, want) {
		t.Errorf("got %q, want %q", all, want)
	}
	docs, err := c.GitLsFiles(root, "--", "*.md")
	if err != nil || !reflect.DeepEqual(docs, []string{"a.md", "dir/c.md"}) {
		t.Errorf("md listing: %q, %v", docs, err)
	}

	writeCorpusFixture(t, root, map[string]string{"late.rs": ""})
	gitRun(t, root, "add", ".")
	if again, _ := c.GitLsFiles(root); !reflect.DeepEqual(again, all) {
		t.Errorf("a second ask re-ran git: %q", again)
	}

	if _, err := c.GitLsFiles(t.TempDir()); err == nil {
		t.Error("want an error outside a git work tree")
	}
}

// Link resolution stats the same targets from several doc scanners.
func TestCorpusExistsRemembersTheAnswer(t *testing.T) {
	root := t.TempDir()
	writeCorpusFixture(t, root, map[string]string{"here.md": ""})
	c := newSourceCorpus()
	here, missing := filepath.Join(root, "here.md"), filepath.Join(root, "missing.md")
	if !c.Exists(here) || c.Exists(missing) || !c.Exists(root) {
		t.Fatal("wrong first answers")
	}
	if err := os.Remove(here); err != nil {
		t.Fatal(err)
	}
	if !c.Exists(here) {
		t.Error("a second ask hit the disk")
	}
}

// Checks run in parallel goroutines, so two scanners asking for the same file
// at once must both get it, with no race (`go test -race` holds this).
func TestCorpusIsSafeForConcurrentScanners(t *testing.T) {
	root := t.TempDir()
	files := map[string]string{}
	for i := range 20 {
		files[filepath.Join("src", strings.Repeat("m", i+1)+".rs")] = strings.Repeat("line\n", i+1)
	}
	writeCorpusFixture(t, root, files)
	c := newSourceCorpus()

	var wg sync.WaitGroup
	errs := make(chan error, 16)
	for range 16 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			errs <- c.WalkDir(filepath.Join(root, "src"), func(path string, d fs.DirEntry, err error) error {
				if err != nil || d.IsDir() {
					return err
				}
				lines, readErr := c.Lines(path)
				if readErr != nil {
					return readErr
				}
				if want := len(filepath.Base(path)) - len(".rs"); len(lines) != want {
					t.Errorf("%s: %d lines, want %d", path, len(lines), want)
				}
				return nil
			})
		}()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Error(err)
		}
	}
}
