package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The locale checks report how many locales they covered. The count is
// structural on purpose: whether a locale is an overlay or a full translation is
// decided once, in TypeScript, because it needs CLDR script data Go can't reach
// (see `nonEnLocaleCount`). So this pins counting, not classification.
func TestNonEnLocaleCountCountsCatalogDirsOnly(t *testing.T) {
	root := t.TempDir()
	messages := filepath.Join(root, "apps", "desktop", "src", "lib", "intl", "messages")
	for _, dir := range []string{"en", "de", "hu", "en-GB", "pt", "pt-PT", "zh", "zh-Hant", "en-XA", "screenshots", "empty"} {
		if err := os.MkdirAll(filepath.Join(messages, dir), 0o755); err != nil {
			t.Fatal(err)
		}
		if dir == "empty" {
			continue // a dir with no JSON isn't a locale
		}
		if err := os.WriteFile(filepath.Join(messages, dir, "app.json"), []byte("{}"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	// Everything with JSON except `en` itself, the non-locale `screenshots/`, and the
	// gitignored pseudolocale `en-XA/` the locale checks leave out of a default run:
	// de, hu, en-GB, pt, pt-PT, zh, zh-Hant.
	if got := nonEnLocaleCount(root); got != 7 {
		t.Errorf("nonEnLocaleCount = %d, want 7", got)
	}
}

func TestNonEnLocaleCountIsZeroWithoutACatalog(t *testing.T) {
	if got := nonEnLocaleCount(t.TempDir()); got != 0 {
		t.Errorf("nonEnLocaleCount on an empty root = %d, want 0", got)
	}
}

// A coverage gap is what new English strings look like until the translator
// agents run, so a LOCAL run only warns (the code commit can come first), while
// CI, the gate in front of `main`, still fails it.
func TestCoverageGapsWarnLocallyAndFailInCI(t *testing.T) {
	const output = "de: 1 key(s) not translated"

	local, err := coverageGapVerdict(false, 1, output)
	if err != nil {
		t.Fatalf("a local run must not fail on a coverage gap, got %v", err)
	}
	if local.Code != ResultWarning {
		t.Fatalf("a local run warns on a coverage gap, got code %v", local.Code)
	}
	if !strings.Contains(local.Message, "translator") {
		t.Fatalf("the local warning names the next step (the translator agents), got %q", local.Message)
	}

	if _, err := coverageGapVerdict(true, 1, output); err == nil {
		t.Fatal("CI must fail on a coverage gap: it's the gate in front of main")
	}
}
