package main

import (
	"os"
	"reflect"
	"strings"
	"testing"
)

// testdata/ci-log-excerpt.txt is lines from v0.55.0's full CI run log (39684718106), as
// `gh run view --log` printed them: color codes as caret text, summaries inline or on the next
// indented line, a linter lane in between, and Playwright's tally outside the check runner.
func TestParseTestLanesReadsTheRealCILog(t *testing.T) {
	f, err := os.Open("testdata/ci-log-excerpt.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = f.Close() }()
	lanes, err := parseTestLanes(f)
	if err != nil {
		t.Fatal(err)
	}
	macOS := "Desktop (Rust, macOS) / Clippy, rustdoc, tests, module cycles"
	want := []testLane{
		{"Desktop (Rust, fixtures)", "rust-integration-tests", "warn", "1 test failed under load, none failed alone: 1 failed an assertion and passed alone, so it's a race to fix (volume::write_test::create_new_checks_then_writes_and_catches_a_writer_mid_upload_on_garage)"},
		{"Desktop (Rust, fixtures)", "webdav-nextcloud", "OK", "12 sabre/dav cells passed"},
		{"Desktop (Svelte)", "svelte-tests", "warn", "17121 tests passed; 1 coverage-allowlist entry look unneeded (coverage ≥ 75%) — remove from coverage-allowlist.json or keep with an updated reason:"},
		{"Desktop (Rust)", "rust-tests", "warn", "2 tests failed under load, none failed alone: 1 passed alone at the same deadline, so the suite was starving it (indexing::reconcile::reconciler::tests::must_scan_routing::queued_rescans_start_after_active_completes); 1 failed an assertion and passed alone, so it's a race to fix (adb::volume_wiring::volume_wiring_test::a_dial_every_joined_attempt_called_off_leaves_nothing_behind)"},
		{"Analytics dashboard", "dashboard-tests", "OK", "All tests passed"},
		{"Scripts (Go)", "go-tests", "OK", "20 packages passed"},
		{"API server", "api-server-tests", "OK", "All tests passed"},
		{"Website", "website-e2e", "OK", "89 tests passed"},
		{macOS, "rust-tests", "OK", "13044 tests passed"},
		{"E2E tests (Playwright, Linux)", "playwright", "OK", "40 skipped, 416 passed"},
	}
	if len(lanes) != len(want) {
		t.Fatalf("got %d lanes, want %d: %+v", len(lanes), len(want), lanes)
	}
	for i := range want {
		if !reflect.DeepEqual(lanes[i], want[i]) {
			t.Errorf("lane %d:\n got %+v\nwant %+v", i, lanes[i], want[i])
		}
	}
}

func TestParseTestLanesStripsRealEscapeBytes(t *testing.T) {
	log := "Website\tstep\t2026-10-21T22:16:01.1Z • 🌐 Website: 🚀 Astro / website-e2e... \x1b[32mOK\x1b[0m (1m) - \x1b[2m89 tests passed\x1b[0m\n"
	lanes, err := parseTestLanes(strings.NewReader(log))
	if err != nil {
		t.Fatal(err)
	}
	if len(lanes) != 1 || lanes[0].status != "OK" || lanes[0].summary != "89 tests passed" {
		t.Errorf("got %+v", lanes)
	}
}

func TestIsTestLane(t *testing.T) {
	for check, want := range map[string]bool{
		"rust-tests": true, "rust-integration-tests": true, "svelte-tests": true, "go-tests": true,
		"website-e2e": true, "webdav-nextcloud": true,
		"clippy": false, "e2e-linux-typecheck": false, "e2e-stale-selector": false, "test-sleep": false, "bare-poll": false,
	} {
		if got := isTestLane(check); got != want {
			t.Errorf("isTestLane(%q) = %v, want %v", check, got, want)
		}
	}
}
