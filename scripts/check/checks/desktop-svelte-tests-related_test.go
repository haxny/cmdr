package checks

import (
	"slices"
	"testing"
)

func TestRelatedTestTargets(t *testing.T) {
	changed := []string{
		"apps/desktop/src/lib/file-explorer/pane/pane-commands.ts",
		"apps/desktop/src/lib/file-explorer/pane/FilePane.svelte",
		"apps/desktop/src/routes/(main)/command-handlers/file-handlers.ts",
		"apps/desktop/scripts/tauri-wrapper.ts",
		"apps/desktop/eslint-plugins/no-raw-lucide-import.js",
		"eslint-plugins/no-error-string-match.js",
		"apps/desktop/src/lib/file-explorer/pane/DETAILS.md",
		"apps/desktop/src-tauri/src/lib.rs",
		"apps/website/src/pages/index.astro",
		"scripts/land/main.go",
		"apps/desktop/src/app.css",
	}
	want := []string{
		"src/lib/file-explorer/pane/pane-commands.ts",
		"src/lib/file-explorer/pane/FilePane.svelte",
		"src/routes/(main)/command-handlers/file-handlers.ts",
		"scripts/tauri-wrapper.ts",
		"eslint-plugins/no-raw-lucide-import.js",
		"../../eslint-plugins/no-error-string-match.js",
	}
	if got := relatedTestTargets(changed); !slices.Equal(got, want) {
		t.Errorf("got  %v\nwant %v", got, want)
	}
}

func TestRelatedTestTargetsDedupesAndSkipsBlankLines(t *testing.T) {
	changed := []string{"", "apps/desktop/src/a.ts", "apps/desktop/src/a.ts"}
	if got := relatedTestTargets(changed); !slices.Equal(got, []string{"src/a.ts"}) {
		t.Errorf("got %v", got)
	}
}
