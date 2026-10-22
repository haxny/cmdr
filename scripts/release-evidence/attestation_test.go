package main

import (
	"os"
	"testing"
)

// testdata/attestation-verify-provenance.json is v0.55.0's provenance check output, trimmed to
// the fields read: two attestations, one from the tag push run and one from the dispatch.
func TestSummarizeAttestations(t *testing.T) {
	out, err := os.ReadFile("testdata/attestation-verify-provenance.json")
	if err != nil {
		t.Fatal(err)
	}
	got, err := summarizeAttestations(out)
	if err != nil {
		t.Fatal(err)
	}
	want := "https://slsa.dev/provenance/v1 over 12 subjects, signed by https://github.com/vdavid/cmdr/.github/workflows/release-pipeline.yml@refs/tags/v0.55.0 from refs/tags/v0.55.0 at 6829d88ff5fae2692bfc684c2fe94d3a58389877, in https://github.com/vdavid/cmdr/actions/runs/39698680663/attempts/1 (github-hosted runner)\n" +
		"https://slsa.dev/provenance/v1 over 10 subjects, signed by https://github.com/vdavid/cmdr/.github/workflows/release-pipeline.yml@refs/tags/v0.55.0 from refs/tags/v0.55.0 at 6829d88ff5fae2692bfc684c2fe94d3a58389877, in https://github.com/vdavid/cmdr/actions/runs/39692184518/attempts/1 (github-hosted runner)"
	if got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
	if _, err := summarizeAttestations([]byte("[]")); err == nil {
		t.Error("no attestation should fail")
	}
}
