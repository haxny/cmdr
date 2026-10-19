package checks

import (
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
)

// corpus is the run's one read of the source tree. Source scanners walk and read
// through it instead of `filepath.WalkDir` / `os.ReadFile` / `bufio.Scanner`, so
// every scanner after the first gets each file without an open or a read.
//
// It's a lazy, process-lifetime memo keyed by absolute path, so a run builds only
// what its selected checks ask for. Each scanner keeps its own walk filtering on
// top. Entries are immutable once built and safe to share across the runner's
// parallel goroutines.
//
// Gotcha: it's a SNAPSHOT. A scanner that must see what an auto-fixer wrote
// earlier in the same run (one that `DependsOn` a formatter) reads from disk
// instead. Why and what it saved: `DETAILS.md` § "The shared source corpus".
var corpus = newSourceCorpus()

type sourceCorpus struct {
	files    sync.Map // absolute path → *corpusFile
	dirs     sync.Map // absolute path → *corpusDir
	exists   sync.Map // absolute path → func() bool
	listings sync.Map // root + "\x00" + args → func() ([]string, error)
}

type corpusFile struct {
	load  func() (string, error)
	lines func() ([]string, error)
}

type corpusDir struct {
	load func() ([]fs.DirEntry, error)
}

func newSourceCorpus() *sourceCorpus {
	return &sourceCorpus{}
}

func (c *sourceCorpus) file(path string) *corpusFile {
	if f, ok := c.files.Load(path); ok {
		return f.(*corpusFile)
	}
	f := &corpusFile{}
	f.load = sync.OnceValues(func() (string, error) {
		raw, err := os.ReadFile(path)
		return string(raw), err
	})
	f.lines = sync.OnceValues(func() ([]string, error) {
		text, err := f.load()
		if err != nil {
			return nil, err
		}
		return splitScannerLines(text), nil
	})
	actual, _ := c.files.LoadOrStore(path, f)
	return actual.(*corpusFile)
}

// ReadFile returns the file's contents, reading the disk only the first time any
// scanner asks. A missing file keeps its `fs.ErrNotExist`.
func (c *sourceCorpus) ReadFile(path string) (string, error) {
	return c.file(path).load()
}

// Lines returns the file split exactly as a `bufio.Scanner` loop splits it: no
// trailing empty line for a final newline, a trailing `\r` dropped. The slice is
// shared, so ❌ never write into it.
func (c *sourceCorpus) Lines(path string) ([]string, error) {
	return c.file(path).lines()
}

func splitScannerLines(text string) []string {
	if text == "" {
		return nil
	}
	lines := strings.Split(text, "\n")
	if lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	for i, line := range lines {
		lines[i] = strings.TrimSuffix(line, "\r")
	}
	return lines
}

// Exists reports whether something is at path (`os.Stat`, so a symlink counts by
// its target), asking the disk once per path.
func (c *sourceCorpus) Exists(path string) bool {
	if f, ok := c.exists.Load(path); ok {
		return f.(func() bool)()
	}
	f, _ := c.exists.LoadOrStore(path, sync.OnceValue(func() bool {
		_, err := os.Stat(path)
		return err == nil
	}))
	return f.(func() bool)()
}

// GitLsFiles is `git -C rootDir ls-files -z <args>`, run once per distinct
// argument list: repo-relative, forward-slashed, in git's order. The slice is
// shared, so ❌ never sort or write into it; copy first. An error means rootDir
// isn't a git work tree (or git failed), so the caller takes its walk fallback.
func (c *sourceCorpus) GitLsFiles(rootDir string, args ...string) ([]string, error) {
	key := rootDir + "\x00" + strings.Join(args, "\x00")
	if f, ok := c.listings.Load(key); ok {
		return f.(func() ([]string, error))()
	}
	f, _ := c.listings.LoadOrStore(key, sync.OnceValues(func() ([]string, error) {
		cmd := exec.Command("git", append([]string{"-C", rootDir, "ls-files", "-z"}, args...)...)
		out, err := cmd.Output()
		if err != nil {
			return nil, fmt.Errorf("git ls-files in %s: %w", rootDir, err)
		}
		var files []string
		for rel := range strings.SplitSeq(string(out), "\x00") {
			if rel != "" {
				files = append(files, rel)
			}
		}
		return files, nil
	}))
	return f.(func() ([]string, error))()
}

// readDir is `os.ReadDir` (sorted entries), remembered per directory.
func (c *sourceCorpus) readDir(path string) ([]fs.DirEntry, error) {
	if d, ok := c.dirs.Load(path); ok {
		return d.(*corpusDir).load()
	}
	d := &corpusDir{load: sync.OnceValues(func() ([]fs.DirEntry, error) { return os.ReadDir(path) })}
	actual, _ := c.dirs.LoadOrStore(path, d)
	return actual.(*corpusDir).load()
}

// WalkDir is `filepath.WalkDir` over remembered directory listings: same visit
// order, same `SkipDir` / `SkipAll` handling, same error callbacks. Only the
// root's own `Lstat` hits the disk on every call.
func (c *sourceCorpus) WalkDir(root string, fn fs.WalkDirFunc) error {
	info, err := os.Lstat(root)
	if err != nil {
		err = fn(root, nil, err)
	} else {
		err = c.walkDir(root, fs.FileInfoToDirEntry(info), fn)
	}
	if err == filepath.SkipDir || err == filepath.SkipAll {
		return nil
	}
	return err
}

func (c *sourceCorpus) walkDir(path string, d fs.DirEntry, fn fs.WalkDirFunc) error {
	// `fs.FileInfoToDirEntry` answers nil only for a nil FileInfo, which a
	// successful Lstat never gives; report it like any unreadable entry.
	if d == nil {
		return fn(path, nil, fs.ErrInvalid)
	}
	if err := fn(path, d, nil); err != nil || !d.IsDir() {
		if err == filepath.SkipDir && d.IsDir() {
			err = nil
		}
		return err
	}
	entries, err := c.readDir(path)
	if err != nil {
		if err = fn(path, d, err); err != nil {
			if err == filepath.SkipDir && d.IsDir() {
				err = nil
			}
			return err
		}
	}
	for _, child := range entries {
		if err := c.walkDir(filepath.Join(path, child.Name()), child, fn); err != nil {
			if err == filepath.SkipDir {
				break
			}
			return err
		}
	}
	return nil
}
