package main

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestParseArgsReadsEveryOptionAndTheCommandAfterTheSeparator(t *testing.T) {
	got, err := parseArgs([]string{"-n", "3", "--edit", "a/b.rs", "copy: starting", "--pre", "cargo clean -p cmdr",
		"--no-warmup", "--", "cargo", "check", "-p", "cmdr"})
	if err != nil {
		t.Fatal(err)
	}
	want := options{runs: 3, editFile: "a/b.rs", editAnchor: "copy: starting", pre: "cargo clean -p cmdr",
		warmup: false, command: []string{"cargo", "check", "-p", "cmdr"}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v, want %+v", got, want)
	}
}

func TestParseArgsDefaultsToFiveRunsWithAWarmUp(t *testing.T) {
	got, err := parseArgs([]string{"--", "true"})
	if err != nil {
		t.Fatal(err)
	}
	if got.runs != 5 || !got.warmup {
		t.Fatalf("got runs %d warmup %v, want 5 and true", got.runs, got.warmup)
	}
}

func TestParseArgsRefusesAnUnknownArgumentAndAMissingCommand(t *testing.T) {
	if _, err := parseArgs([]string{"--frobnicate", "--", "true"}); err == nil {
		t.Fatal("an unknown argument was accepted")
	}
	if _, err := parseArgs([]string{"-n", "2"}); err == nil {
		t.Fatal("a missing command was accepted")
	}
	if _, err := parseArgs([]string{"--edit", "only-a-file"}); err == nil {
		t.Fatal("--edit with one value was accepted")
	}
}

func TestSetMarkerRewritesTheFirstAnchorAndRestoresIt(t *testing.T) {
	path := filepath.Join(t.TempDir(), "mod.rs")
	original := `log::debug!("copy: starting {}"); log::debug!("copy: starting again");`
	if err := os.WriteFile(path, []byte(original), 0o644); err != nil {
		t.Fatal(err)
	}

	mustSetMarker(t, path, "copy: starting", " [bench 1]")
	assertFile(t, path, `log::debug!("copy: starting [bench 1] {}"); log::debug!("copy: starting again");`)

	mustSetMarker(t, path, "copy: starting", " [bench 12]")
	assertFile(t, path, `log::debug!("copy: starting [bench 12] {}"); log::debug!("copy: starting again");`)

	mustSetMarker(t, path, "copy: starting", "")
	assertFile(t, path, original)
}

func TestSetMarkerTreatsTheAnchorAsLiteralText(t *testing.T) {
	path := filepath.Join(t.TempDir(), "mod.rs")
	if err := os.WriteFile(path, []byte(`"a.b (c)" and "axb (c)"`), 0o644); err != nil {
		t.Fatal(err)
	}
	mustSetMarker(t, path, "a.b (c)", " [bench 2]")
	assertFile(t, path, `"a.b (c) [bench 2]" and "axb (c)"`)
}

func TestStatsGivesTheMedianOfAnOddAndAnEvenCount(t *testing.T) {
	if got := stats([]float64{3, 1, 2}); got != "median 2.00s  min 1.00s  max 3.00s" {
		t.Fatalf("odd: got %q", got)
	}
	if got := stats([]float64{4, 1, 2, 3}); got != "median 2.50s  min 1.00s  max 4.00s" {
		t.Fatalf("even: got %q", got)
	}
}

func TestLoadAverageDropsTheBracesSysctlPrints(t *testing.T) {
	if got := parseLoadAverage("{ 1.23 2.34 3.45 }\n"); got != "1.23 2.34 3.45" {
		t.Fatalf("got %q", got)
	}
}

func mustSetMarker(t *testing.T, path, anchor, suffix string) {
	t.Helper()
	if err := setMarker(path, anchor, suffix); err != nil {
		t.Fatal(err)
	}
}

func assertFile(t *testing.T, path, want string) {
	t.Helper()
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != want {
		t.Fatalf("file holds %q, want %q", got, want)
	}
}
