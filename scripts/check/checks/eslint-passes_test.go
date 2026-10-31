package checks

import (
	"slices"
	"strings"
	"testing"
)

// Regression anchor for #622: a type-aware rule judges code against the TypeScript program, so
// while the tree doesn't typecheck (another agent mid-edit in a shared worktree) it misjudges
// correct code, and `--fix` then rewrites it: stripped `void` reads from a `$effect`, a deleted
// live `eslint-disable`, a dropped `as` cast. So `--fix` may only ever run with type-aware rules
// off (ESLINT_NO_TYPECHECK=1) and with directive fixes excluded, in every lane and mode.
func assertNoTypeAwareFix(t *testing.T, name string, passes []eslintPass) {
	t.Helper()
	for i, p := range passes {
		if !slices.Contains(p.args, "--fix") {
			continue
		}
		if !p.noTypecheck {
			t.Errorf("%s pass %d runs --fix with type-aware rules on: %v", name, i, p.args)
		}
		idx := slices.Index(p.args, "--fix-type")
		if idx < 0 || idx+1 >= len(p.args) {
			t.Errorf("%s pass %d runs --fix without a --fix-type allowlist, so it may rewrite eslint-disable directives: %v", name, i, p.args)
			continue
		}
		if slices.Contains(strings.Split(p.args[idx+1], ","), "directive") {
			t.Errorf("%s pass %d lets --fix rewrite eslint-disable directives: %v", name, i, p.args)
		}
	}
}

func TestESLintPassesNeverFixWithTypeAwareRules(t *testing.T) {
	for _, ci := range []bool{false, true} {
		assertNoTypeAwareFix(t, "appESLintPasses", appESLintPasses(ci))
		assertNoTypeAwareFix(t, "typeAwareESLintPasses", typeAwareESLintPasses([]string{"."}))
		assertNoTypeAwareFix(t, "typeAwareFixPasses", typeAwareFixPasses([]string{"."}, ci))
	}
}

// The desktop's two type-aware lanes own its whole ESLint coverage, so the local fix pass lives
// in them, scoped to the lane's own file set: the Svelte and TypeScript lanes run in parallel, and
// two fix passes over the same files would rewrite each other's work.
func TestTypeAwareFixPassesStayInTheirScope(t *testing.T) {
	scope := []string{".", "--ignore-pattern", "**/*.svelte"}
	fix := typeAwareFixPasses(scope, false)
	if len(fix) != 1 {
		t.Fatalf("want one local fix pass, got %+v", fix)
	}
	if !slices.Equal(fix[0].args[:len(scope)], scope) {
		t.Errorf("the fix pass must lint exactly the lane's scope %v, got %v", scope, fix[0].args)
	}
}

// The type-aware rules must still RUN somewhere, check-only, or the guard above would pass by
// linting nothing.
func TestESLintPassesStillRunTypeAwareRules(t *testing.T) {
	for _, ci := range []bool{false, true} {
		for name, passes := range map[string][]eslintPass{
			"appESLintPasses":       appESLintPasses(ci),
			"typeAwareESLintPasses": typeAwareESLintPasses([]string{"."}),
		} {
			last := passes[len(passes)-1]
			if last.noTypecheck || slices.Contains(last.args, "--fix") {
				t.Errorf("%s (ci=%v): the final pass must be the full config, check-only: %+v", name, ci, last)
			}
		}
	}
}

// Local runs keep auto-fixing what's safe (the non-type-aware rules); CI never fixes.
func TestESLintPassesAutoFixLocallyOnly(t *testing.T) {
	for name, plan := range map[string]func(ci bool) []eslintPass{
		"appESLintPasses":    appESLintPasses,
		"typeAwareFixPasses": func(ci bool) []eslintPass { return typeAwareFixPasses([]string{"."}, ci) },
	} {
		if got := plan(false); !slices.Contains(got[0].args, "--fix") {
			t.Errorf("%s: a local run should start with a fix pass, got %+v", name, got)
		}
		for _, p := range plan(true) {
			if slices.Contains(p.args, "--fix") {
				t.Errorf("%s: CI must never fix, got %+v", name, p)
			}
		}
	}
}
