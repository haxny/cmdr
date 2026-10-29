package main

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func TestMatchPattern(t *testing.T) {
	cases := []struct {
		pattern, file string
		want          bool
	}{
		{"apps/desktop/src-tauri/src/updater/**", "apps/desktop/src-tauri/src/updater/mod.rs", true},
		{"apps/desktop/src-tauri/src/updater/**", "apps/desktop/src-tauri/src/updater/a/b/c.rs", true},
		{"apps/desktop/src-tauri/src/updater/**", "apps/desktop/src-tauri/src/updaters/mod.rs", false},
		{"**/tauri.conf.json", "apps/desktop/src-tauri/tauri.conf.json", true},
		{"**/tauri.conf.json", "tauri.conf.json", true},
		{"apps/api-server/src/*.ts", "apps/api-server/src/index.ts", true},
		{"apps/api-server/src/*.ts", "apps/api-server/src/admin/admin.ts", false},
		{"apps/api-server/**/*.ts", "apps/api-server/src/admin/admin.ts", true},
		{"SECURITY.md", "SECURITY.md", true},
		{"SECURITY.md", "docs/SECURITY.md", false},
		{"crates/cmdr-*/src/**", "crates/cmdr-smb/src/lib.rs", true},
	}
	for _, c := range cases {
		if got := matchPattern(c.pattern, c.file); got != c.want {
			t.Errorf("matchPattern(%q, %q) = %v, want %v", c.pattern, c.file, got, c.want)
		}
	}
}

func TestMatchingSkipsDocsAndTestsUnlessNamed(t *testing.T) {
	s := section{Paths: []string{"apps/desktop/src-tauri/src/ai/**", "apps/desktop/test/e2e-playwright/accessibility.spec.ts"}}
	got := s.matching([]string{
		"apps/desktop/src-tauri/src/ai/client.rs",
		"apps/desktop/src-tauri/src/ai/CLAUDE.md",
		"apps/desktop/src-tauri/src/ai/client_test.rs",
		"apps/desktop/src-tauri/src/ai/tests/mod.rs",
		"apps/desktop/test/e2e-playwright/accessibility.spec.ts",
	})
	want := []string{"apps/desktop/src-tauri/src/ai/client.rs", "apps/desktop/test/e2e-playwright/accessibility.spec.ts"}
	if !slices.Equal(got, want) {
		t.Errorf("matching = %v, want %v", got, want)
	}
}

func TestParseVerifiedAgainst(t *testing.T) {
	got, err := parseVerifiedAgainst("/** x */\nexport const verifiedAgainst = { version: '0.57.0', date: '2026-10-29' }\n")
	if err != nil {
		t.Fatal(err)
	}
	if got.version != "0.57.0" || got.date != "2026-10-29" {
		t.Errorf("got %+v", got)
	}
	if _, err := parseVerifiedAgainst("export const verifiedAgainst = { version: '0.57' }"); err == nil {
		t.Error("expected an error for a malformed line")
	}
}

func TestHeadingIDsSkipsComputedOnes(t *testing.T) {
	src := `<h2 id="summary">Summary</h2>
<h2 class="x" id="network">Network</h2>
<h3 id="allowlist">Hosts</h3>
<h2 id={standard.id}>{standard.title}</h2>`
	got := headingIDs(src)
	if !slices.Equal(got, []string{"summary", "network"}) {
		t.Errorf("headingIDs = %v", got)
	}
}

func TestValidateReportsEveryDrift(t *testing.T) {
	root := t.TempDir()
	page := filepath.Join(root, "apps/website/src/pages/trust.astro")
	if err := os.MkdirAll(filepath.Dir(page), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(page, []byte(`<h2 id="summary">S</h2><h2 id="network">N</h2>`), 0o644); err != nil {
		t.Fatal(err)
	}
	m := sourceMap{Sections: []section{
		{Page: "/trust", Anchor: "summary", Paths: []string{"SECURITY.md"}},
		{Page: "/trust", Anchor: "gone", Paths: []string{"nothing/**"}},
		{Page: "/trust/missing", Anchor: "x", Paths: []string{"SECURITY.md"}},
	}}
	problems, err := m.validate(root, []string{"SECURITY.md", "apps/website/src/pages/trust.astro"})
	if err != nil {
		t.Fatal(err)
	}
	joined := strings.Join(problems, "\n")
	for _, want := range []string{
		"/trust#gone: no such <h2 id>",
		`/trust#gone: "nothing/**" matches no tracked file`,
		"/trust#network: heading has no entry",
		"/trust/missing#x: no page at",
	} {
		if !strings.Contains(joined, want) {
			t.Errorf("missing problem %q in:\n%s", want, joined)
		}
	}
	if strings.Contains(joined, "/trust#summary") {
		t.Errorf("a valid entry was reported:\n%s", joined)
	}
}
