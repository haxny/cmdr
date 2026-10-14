package main

import (
	"os"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

// Real names from a worktree's `target/debug/deps` (2026-10-14). An incremental
// unit writes `<unit>.<cgu>.<session>.rcgu.o`; a non-incremental one (registry
// crates) writes `<unit>.<crate>.<hash>-cgu.<n>.rcgu.o`.
func TestRcguGenerationParsesBothNameShapes(t *testing.T) {
	cases := []struct {
		name, unit, gen string
		ok              bool
	}{
		{"cmdr_webdav-5792f38d0b38b562.04a362u2csda9pd40g0dyjl4o.184vgut.rcgu.o", "cmdr_webdav-5792f38d0b38b562", "184vgut", true},
		{"cmdr_lib.00s8zplx6nhn9s4ab87wsmrnk.0q0dlua.rcgu.o", "cmdr_lib", "0q0dlua", true},
		{"adler2-14be92afa4db7125.adler2.f82e415231b04e2a-cgu.0.rcgu.o", "adler2-14be92afa4db7125", "adler2.f82e415231b04e2a-cgu", true},
		{"cmdr_webdav-5792f38d0b38b562", "", "", false},
		{"cmdr_webdav-5792f38d0b38b562.d", "", "", false},
		{"weird.rcgu.o", "", "", false},
	}
	for _, c := range cases {
		unit, gen, ok := rcguGeneration(c.name)
		if unit != c.unit || gen != c.gen || ok != c.ok {
			t.Errorf("rcguGeneration(%q) = (%q, %q, %v), want (%q, %q, %v)", c.name, unit, gen, ok, c.unit, c.gen, c.ok)
		}
	}
}

func obj(unit, cgu, gen string, mtime time.Time) rcguFile {
	return rcguFile{name: unit + "." + cgu + "." + gen + ".rcgu.o", mtime: mtime, size: 100}
}

func names(files []rcguFile) []string {
	out := make([]string, 0, len(files))
	for _, f := range files {
		out = append(out, f.name)
	}
	sort.Strings(out)
	return out
}

func TestStaleRcguObjectsDropsSupersededGenerations(t *testing.T) {
	now := time.Date(2026, 10, 14, 12, 0, 0, 0, time.UTC)
	old := now.Add(-48 * time.Hour)
	newer := now.Add(-3 * time.Hour)
	files := []rcguFile{
		obj("app-1", "a", "gen1", old),
		obj("app-1", "b", "gen1", old),
		obj("app-1", "a", "gen2", newer),
		// A CGU reused from the incremental cache is a hard link, so it keeps the
		// mtime of the session that first wrote it. The generation still counts as
		// new through its freshest member.
		obj("app-1", "b", "gen2", old),
		// A unit with one generation is the live one, however old.
		obj("lib-2", "x", "only", old),
	}

	got := names(staleRcguObjects(files, now))
	want := []string{"app-1.a.gen1.rcgu.o", "app-1.b.gen1.rcgu.o"}
	if len(got) != len(want) || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("stale = %v, want %v", got, want)
	}
}

// A generation superseded moments ago may still back a binary a running test
// or a concurrent cargo is using, so it stays until the newer one has settled.
func TestStaleRcguObjectsKeepsGenerationsSupersededWithinTheGrace(t *testing.T) {
	now := time.Date(2026, 10, 14, 12, 0, 0, 0, time.UTC)
	files := []rcguFile{
		obj("app-1", "a", "gen1", now.Add(-48*time.Hour)),
		obj("app-1", "a", "gen2", now.Add(-time.Minute)),
	}
	if got := staleRcguObjects(files, now); len(got) != 0 {
		t.Fatalf("stale = %v, want none: gen2 is younger than the grace", names(got))
	}

	// With a third, settled generation in between, gen1 goes and gen2 (settled)
	// survives alongside the fresh gen3.
	files = []rcguFile{
		obj("app-1", "a", "gen1", now.Add(-48*time.Hour)),
		obj("app-1", "a", "gen2", now.Add(-time.Hour)),
		obj("app-1", "a", "gen3", now.Add(-time.Minute)),
	}
	got := names(staleRcguObjects(files, now))
	if len(got) != 1 || got[0] != "app-1.a.gen1.rcgu.o" {
		t.Fatalf("stale = %v, want only gen1", got)
	}
}

// Generations tied on their freshest mtime are hard links to the same work
// products; neither is provably older, so both stay.
func TestStaleRcguObjectsKeepsTiedGenerations(t *testing.T) {
	now := time.Date(2026, 10, 14, 12, 0, 0, 0, time.UTC)
	same := now.Add(-5 * time.Hour)
	files := []rcguFile{obj("app-1", "a", "gen1", same), obj("app-1", "a", "gen2", same)}
	if got := staleRcguObjects(files, now); len(got) != 0 {
		t.Fatalf("stale = %v, want none", names(got))
	}
}

func TestPruneStaleRcguObjectsDeletesOnlyStaleObjects(t *testing.T) {
	dir := t.TempDir()
	now := time.Now()
	write := func(name string, mtime time.Time) {
		path := filepath.Join(dir, name)
		if err := os.WriteFile(path, []byte("0123456789"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.Chtimes(path, mtime, mtime); err != nil {
			t.Fatal(err)
		}
	}
	write("app-1.a.gen1.rcgu.o", now.Add(-48*time.Hour))
	write("app-1.a.gen2.rcgu.o", now.Add(-2*time.Hour))
	write("app-1", now.Add(-48*time.Hour)) // the binary itself
	write("app-1.d", now.Add(-48*time.Hour))
	write("lib-2.x.only.rcgu.o", now.Add(-48*time.Hour))

	res, err := pruneStaleRcguObjects(dir, now)
	if err != nil {
		t.Fatal(err)
	}
	if res.files != 1 || res.bytes != 10 {
		t.Fatalf("result = %+v, want 1 file / 10 bytes", res)
	}
	entries, _ := os.ReadDir(dir)
	var left []string
	for _, e := range entries {
		left = append(left, e.Name())
	}
	sort.Strings(left)
	want := []string{"app-1", "app-1.a.gen2.rcgu.o", "app-1.d", "lib-2.x.only.rcgu.o"}
	if len(left) != len(want) {
		t.Fatalf("left = %v, want %v", left, want)
	}
	for i := range want {
		if left[i] != want[i] {
			t.Fatalf("left = %v, want %v", left, want)
		}
	}
}

// A target dir that was never built (CI's first run, a fresh clone) is a no-op.
func TestPruneStaleRcguObjectsToleratesAMissingDir(t *testing.T) {
	res, err := pruneStaleRcguObjects(filepath.Join(t.TempDir(), "nope"), time.Now())
	if err != nil || res.files != 0 {
		t.Fatalf("got (%+v, %v), want a silent no-op", res, err)
	}
}
