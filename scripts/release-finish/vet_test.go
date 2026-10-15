package main

import (
	"errors"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

// The gate runs the app's own check through `cmdr-vet-update`, on every archive, at the release's
// version, with the lockfile as committed.
func TestVetRunsTheAppsCheckOnEveryArchive(t *testing.T) {
	var gotDir, gotName string
	var gotArgs []string
	run := func(dir, name string, args ...string) (string, error) {
		gotDir, gotName, gotArgs = dir, name, args
		return "", nil
	}
	names := updateArchiveNames("0.53.0")
	if err := vetArchives(run, "/repo", "0.53.0", "/work", names); err != nil {
		t.Fatalf("want a pass, got %v", err)
	}
	if gotDir != "/repo" || gotName != "cargo" {
		t.Fatalf("ran %s in %s, want cargo in /repo", gotName, gotDir)
	}
	want := []string{"run", "--quiet", "--release", "--locked", "-p", "cmdr-update-trust", "--bin", "cmdr-vet-update", "--",
		"--version", "0.53.0"}
	for _, name := range names {
		want = append(want, filepath.Join("/work", name))
	}
	if !slices.Equal(gotArgs, want) {
		t.Fatalf("args %v, want %v", gotArgs, want)
	}
}

// ❗ An archive the updater would refuse must never get signed: signing it would publish a release
// every install refuses to take, and no later update could reach them.
func TestVetRefusesWhenTheCheckRefuses(t *testing.T) {
	run := func(string, string, ...string) (string, error) {
		return "    Cmdr_0.53.0_x64.app.tar.gz: REFUSED, the bundle isn't signed by Cmdr's Developer ID", errors.New("exit status 1")
	}
	err := vetArchives(run, "/repo", "0.53.0", "/work", updateArchiveNames("0.53.0"))
	if err == nil || !strings.Contains(err.Error(), "refusing to sign") {
		t.Fatalf("want a refusal to sign, got %v", err)
	}
}
