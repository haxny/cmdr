package checks

import (
	"testing"
	"time"
)

// osvReportFixture is trimmed from a real `osv-scanner scan source -r --format json` run
// (v2.6.0, 2026-10-13): one crate with a fixed advisory, grouped with its GHSA alias.
const osvReportFixture = `{
  "results": [
    {
      "source": {"path": "/tmp/export/fuzz/Cargo.lock", "type": "lockfile"},
      "packages": [
        {
          "package": {"name": "quinn-proto", "version": "0.11.16", "ecosystem": "crates.io"},
          "groups": [{"ids": ["RUSTSEC-2026-0338"], "aliases": ["GHSA-465w-v9q3-7j98", "RUSTSEC-2026-0338"], "max_severity": ""}],
          "vulnerabilities": [
            {
              "id": "RUSTSEC-2026-0338",
              "published": "2026-09-14T12:00:00Z",
              "summary": "Remote peer can panic quinn-proto",
              "affected": [
                {
                  "package": {"ecosystem": "crates.io", "name": "quinn-proto"},
                  "ranges": [{"type": "SEMVER", "events": [{"introduced": "0.7.0"}, {"fixed": "0.11.18"}]}]
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}`

func TestParseOSVReport(t *testing.T) {
	findings, err := parseOSVReport([]byte(osvReportFixture), "/tmp/export")
	if err != nil {
		t.Fatal(err)
	}
	if len(findings) != 1 {
		t.Fatalf("want one finding per group, got %d: %+v", len(findings), findings)
	}
	f := findings[0]
	if f.where != "fuzz/Cargo.lock" {
		t.Errorf("where = %q, want the lockfile relative to the scanned root", f.where)
	}
	a := f.advisory
	want := vulnAdvisory{
		ID: "RUSTSEC-2026-0338", Summary: "Remote peer can panic quinn-proto",
		Published: time.Date(2026, 9, 14, 12, 0, 0, 0, time.UTC),
		Ecosystem: "crates.io", Module: "quinn-proto", Version: "0.11.16", FixedVersion: "0.11.18",
	}
	if a != want {
		t.Errorf("advisory =\n  %+v\nwant\n  %+v", a, want)
	}
}

func TestOSVFixedVersion(t *testing.T) {
	ranges := []osvRange{
		{Type: "SEMVER", Events: []osvEvent{{Introduced: "0"}, {Fixed: "0.9.5"}, {Introduced: "0.10.0"}, {Fixed: "0.10.3"}}},
	}
	cases := []struct {
		version, want string
	}{
		{"0.9.1", "0.9.5"},
		{"0.10.2", "0.10.3"},
		{"0.10.3", ""}, // at the fix: not in this range
		{"0.9.7", ""},  // between ranges: unaffected, so no fix to name
	}
	for _, tc := range cases {
		if got := osvFixedVersion(ranges, tc.version); got != tc.want {
			t.Errorf("osvFixedVersion(%s) = %q, want %q", tc.version, got, tc.want)
		}
	}

	lastAffectedOnly := []osvRange{{Type: "ECOSYSTEM", Events: []osvEvent{{Introduced: "0"}, {LastAffected: "2.1.0"}}}}
	if got := osvFixedVersion(lastAffectedOnly, "2.0.0"); got != "" {
		t.Errorf("a range with no fix event must yield no fix, got %q", got)
	}
	gitRange := []osvRange{{Type: "GIT", Events: []osvEvent{{Introduced: "0"}, {Fixed: "abc123"}}}}
	if got := osvFixedVersion(gitRange, "1.0.0"); got != "" {
		t.Errorf("a GIT range names commits, not versions, got %q", got)
	}
}

func TestCompareLooseVersions(t *testing.T) {
	cases := []struct {
		a, b string
		want int
	}{
		{"0.11.16", "0.11.18", -1},
		{"1.10.0", "1.9.0", 1},
		{"v1.2.3", "1.2.3", 0},
		{"1.2", "1.2.0", 0},
		{"1.0.0-rc.1", "1.0.0", -1},
		{"1.0.0+build.5", "1.0.0", 0},
		{"0", "0.7.0", -1},
	}
	for _, tc := range cases {
		if got := compareLooseVersions(tc.a, tc.b); got != tc.want {
			t.Errorf("compareLooseVersions(%s, %s) = %d, want %d", tc.a, tc.b, got, tc.want)
		}
	}
}
