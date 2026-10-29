package checks

import (
	"math/rand/v2"
	"strings"
	"testing"
)

// testDynamicPrefixes stands in for the real allowlist so the pure logic can be
// exercised independently of the production prefix set.
var testDynamicPrefixes = []string{"errors.git.", "errors.listing."}

func TestFindUnusedMessageKeys_FlagsKeyWithNoMention(t *testing.T) {
	catalog := []string{"common.cancel", "settings.fsWatch.title", "menu.open.never"}
	// Only the first two literals appear anywhere in source.
	sources := []string{
		`const x = t('common.cancel')`,
		`<button>{t('settings.fsWatch.title')}</button>`,
	}
	unused := findUnusedMessageKeys(catalog, sources, nil)
	if len(unused) != 1 || unused[0] != "menu.open.never" {
		t.Fatalf("got %#v, want exactly [menu.open.never]", unused)
	}
}

func TestFindUnusedMessageKeys_AllReferencedIsClean(t *testing.T) {
	catalog := []string{"common.cancel", "common.ok"}
	sources := []string{`t('common.cancel'); t('common.ok')`}
	if unused := findUnusedMessageKeys(catalog, sources, nil); len(unused) != 0 {
		t.Fatalf("got %#v, want none", unused)
	}
}

func TestFindUnusedMessageKeys_CountsIndirectMention(t *testing.T) {
	// A key reached through a registry map (its literal stored in a `*Key` field)
	// or any other indirection still has its literal text in source, so a
	// substring mention counts as a reference.
	catalog := []string{"settings.tint.red"}
	sources := []string{`const tintKeys = { red: 'settings.tint.red' }`}
	if unused := findUnusedMessageKeys(catalog, sources, nil); len(unused) != 0 {
		t.Fatalf("got %#v, want none (indirect mention counts)", unused)
	}
}

func TestFindUnusedMessageKeys_AllowlistedDynamicPrefixNotFlagged(t *testing.T) {
	// Keys built at runtime never appear as a literal, so a naive scan would flag
	// them. An allowlisted dynamic prefix treats every key under it as referenced.
	catalog := []string{
		"errors.git.notARepo.title",
		"errors.listing.notFound.title",
		"menu.open.never", // not under any allowlisted prefix → still flagged
	}
	sources := []string{`const noop = true`}
	unused := findUnusedMessageKeys(catalog, sources, testDynamicPrefixes)
	if len(unused) != 1 || unused[0] != "menu.open.never" {
		t.Fatalf("got %#v, want exactly [menu.open.never]", unused)
	}
}

func TestFindUnusedMessageKeys_ResultIsSorted(t *testing.T) {
	catalog := []string{"zeta.b.leaf", "alpha.a.leaf", "mid.c.leaf"}
	unused := findUnusedMessageKeys(catalog, []string{`noop`}, nil)
	want := []string{"alpha.a.leaf", "mid.c.leaf", "zeta.b.leaf"}
	if len(unused) != len(want) {
		t.Fatalf("got %#v, want %#v", unused, want)
	}
	for i := range want {
		if unused[i] != want[i] {
			t.Fatalf("got %#v, want sorted %#v", unused, want)
		}
	}
}

// Guards against a prefix that no longer corresponds to any catalog key: a stale
// allowlist entry must surface so the dynamic-prefix list stays honest (each
// entry tied to a real runtime-construction site that still has keys).
func TestUnusedKeyPrefixCoverage_ReportsStaleEntry(t *testing.T) {
	catalog := []string{"errors.git.notARepo.title"}
	prefixes := []string{"errors.git.", "errors.gone."}
	stale := unusedDynamicPrefixesWithoutKeys(catalog, prefixes)
	if len(stale) != 1 || stale[0] != "errors.gone." {
		t.Fatalf("got %#v, want exactly [errors.gone.]", stale)
	}
}

// The scan is one pass per file over every catalog key at once (an Aho-Corasick automaton),
// replacing a `strings.Contains` per key per file that cost ~20 s on the real tree. Its verdict
// must stay exactly the substring semantics: a key counts wherever its text appears, inside a
// longer dotted path, mid-identifier, or overlapping another key.
// mentionedInAny is the reference semantics the matcher must reproduce.
func mentionedInAny(key string, sources []string) bool {
	for _, src := range sources {
		if strings.Contains(src, key) {
			return true
		}
	}
	return false
}

func TestKeyMentionMatcher_MatchesSubstringSemantics(t *testing.T) {
	keys := []string{"a.b", "a.b.c", "b.c", "x.a.b", "settings.tint.red", "c", "never.here", "a.b"}
	sources := []string{
		`t('a.b.c')`,           // a.b, a.b.c, b.c, and c all occur
		`const k = 'zx.a.bq'`,  // x.a.b mid-identifier
		`settings.tint.redish`, // a prefix of a longer word still counts
	}
	got := newKeyMentionMatcher(keys).mentioned(sources)
	for _, k := range keys {
		if want := mentionedInAny(k, sources); got[k] != want {
			t.Errorf("key %q: matcher says %v, substring scan says %v", k, got[k], want)
		}
	}
}

func TestKeyMentionMatcher_AgreesWithSubstringScanOnRandomInput(t *testing.T) {
	rng := rand.New(rand.NewPCG(1, 2))
	const alphabet = "ab.c_"
	word := func(n int) string {
		b := make([]byte, n)
		for i := range b {
			b[i] = alphabet[rng.IntN(len(alphabet))]
		}
		return string(b)
	}
	for round := range 500 {
		keys := make([]string, 1+rng.IntN(12))
		for i := range keys {
			keys[i] = word(1 + rng.IntN(5))
		}
		sources := make([]string, rng.IntN(4))
		for i := range sources {
			sources[i] = word(rng.IntN(40)) + " 'é" + word(rng.IntN(8)) + "'"
		}
		got := newKeyMentionMatcher(keys).mentioned(sources)
		for _, k := range keys {
			if want := mentionedInAny(k, sources); got[k] != want {
				t.Fatalf("round %d, key %q in %q: matcher says %v, substring scan says %v", round, k, sources, got[k], want)
			}
		}
	}
}

func TestUnusedKeyPrefixCoverage_AllPrefixesCovered(t *testing.T) {
	catalog := []string{"errors.git.notARepo.title", "errors.listing.notFound.title"}
	if stale := unusedDynamicPrefixesWithoutKeys(catalog, testDynamicPrefixes); len(stale) != 0 {
		t.Fatalf("got %#v, want none", stale)
	}
}
