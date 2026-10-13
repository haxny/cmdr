package main

import "testing"

// A scoped run used to skip every whole-repo lane, so a green `pnpm check svelte`
// could hide a dead doc link or an over-budget CLAUDE.md that only a bare run, or
// CI, caught. The cheap doc and size lanes now join it; the formatters don't,
// since a local run rewrites files and the git hooks own formatting.
func TestSelectChecks_ScopedRunsBringTheDocLanes(t *testing.T) {
	scoped := []*cliFlags{
		{svelteOnly: true},
		{appNames: []string{"desktop"}},
		{checkNames: []string{"clippy"}},
	}
	for _, flags := range scoped {
		ids := selectedIDs(t, flags)
		for _, want := range []string{"docs-dead-links", "claude-md-length", "file-length"} {
			if !ids[want] {
				t.Errorf("%+v: a scoped run left out %s", *flags, want)
			}
		}
		if ids["oxfmt"] {
			t.Errorf("%+v: a scoped run pulled in a formatter", *flags)
		}
	}
}

// CI's jobs name their checks, and the doc lanes have a job of their own, so a CI
// run gets exactly what it names. So does `--exact`, which the pre-push hook's
// notices step uses: a doc-lane failure there mustn't read as "the runner didn't
// finish" and skip the regeneration.
func TestSelectChecks_CIAndExactRunsStayExact(t *testing.T) {
	for _, flags := range []*cliFlags{
		{checkNames: []string{"clippy"}, ciMode: true},
		{checkNames: []string{"clippy"}, exact: true},
	} {
		if ids := selectedIDs(t, flags); len(ids) != 1 {
			t.Errorf("%+v: selected %d checks, want only clippy", *flags, len(ids))
		}
	}
}

// A bare run already has every lane; the doc lanes must not be added twice.
func TestSelectChecks_ABareRunIsUnchanged(t *testing.T) {
	if ids := selectedIDs(t, &cliFlags{}); !ids["oxfmt"] || !ids["docs-dead-links"] {
		t.Error("a bare run lost a whole-repo lane")
	}
}

func selectedIDs(t *testing.T, flags *cliFlags) map[string]bool {
	t.Helper()
	selected, err := selectChecks(flags)
	if err != nil {
		t.Fatalf("selectChecks() returned error: %v", err)
	}
	ids := make(map[string]bool, len(selected))
	for _, c := range selected {
		if ids[c.ID] {
			t.Errorf("check %q selected twice", c.ID)
		}
		ids[c.ID] = true
	}
	return ids
}
