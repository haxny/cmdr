package checks

import (
	"slices"
	"strings"
	"testing"
)

// Shaped like real `cargo doc` output, which is the whole point: cargo's own
// progress line runs straight into the first diagnostic with no blank line, and
// the closing `error: could not document` is glued to the warning count above it.
// A fixture with a blank line everywhere lets a paragraph-based filter pass while
// the real thing drops the diagnostic the check exists to show.
const realRustdocOutput = ` Documenting cmdr-index v0.0.0 (/repo/crates/cmdr-index)
warning: unowned lint nobody denied yet
  --> crates/cmdr-index/src/indexing/host/config.rs:16:65
   |
16 | //! [` + "`IndexConfig`" + `] is an INPUT value. [` + "`set_config`" + `] pushes
   |                                                     ^^^^^^^^^^ this item is private
   |
   = note: a note line

error: unresolved link to ` + "`limit`" + `
  --> crates/cmdr-index/src/importance/scheduler/mod.rs:45:26
   |
45 | //!   run -- <db> [limit]
   |                    ^^^^^ no item named ` + "`limit`" + ` in scope
   |
   = help: to escape ` + "`[`" + ` and ` + "`]`" + ` characters, add a backslash before them

 Finished ` + "`dev`" + ` profile in 12.3s
error: could not document ` + "`cmdr-index`" + `
`

func TestRustdocOutputKeepsEveryDiagnosticWhole(t *testing.T) {
	kept := rustdocDiagnostics(realRustdocOutput)
	// Both severities survive: a warning is a lint nobody owns, which the check
	// refuses to pass on just like an error.
	for _, line := range []string{"unowned lint nobody denied yet", "= note: a note line"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the warning block lost %q:\n%s", line, kept)
		}
	}
	// Every line of the error block survives, including the source excerpt and the
	// help note — the diagnostic is useless without them.
	for _, line := range []string{"unresolved link to", "scheduler/mod.rs:45:26", "no item named", "= help: to escape", "could not document"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the error block lost %q:\n%s", line, kept)
		}
	}
	// Cargo's progress lines are noise, wherever they sit. The first one opens the
	// stream and the second is glued between two diagnostics.
	for _, noise := range []string{"Documenting", "Finished"} {
		if strings.Contains(kept, noise) {
			t.Fatalf("cargo's %q progress line is not a diagnostic:\n%s", noise, kept)
		}
	}
}

func TestRustdocOutputKeepsADiagnosticThatOpensTheStream(t *testing.T) {
	// The failure that made a paragraph-based filter useless: the one error was the
	// FIRST diagnostic, so a paragraph split glued it to cargo's progress line and
	// no block started with `error`.
	output := ` Documenting cmdr-index v0.0.0 (/repo/crates/cmdr-index)
error: unresolved link to ` + "`limit`" + `
  --> crates/cmdr-index/src/importance/scheduler/mod.rs:45:26
   |
45 | //!   run -- <db> [limit]
   |                    ^^^^^ no item named ` + "`limit`" + ` in scope
`
	kept := rustdocDiagnostics(output)
	if strings.Contains(kept, "Documenting") {
		t.Fatalf("cargo's progress line came back with the error:\n%s", kept)
	}
	for _, line := range []string{"unresolved link to", "no item named"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the error block lost %q:\n%s", line, kept)
		}
	}
}

func TestRustdocCleanOutputHasNoDiagnostics(t *testing.T) {
	// The green path leans on this: anything left over means a lint fired, so a
	// clean stream must parse to the empty string rather than to its noise.
	clean := " Documenting cmdr v0.38.0 (/repo/apps/desktop/src-tauri)\n Finished `dev` profile in 41.2s\n"
	if got := rustdocDiagnostics(clean); got != "" {
		t.Fatalf("a clean run must parse to nothing, got %q", got)
	}
}

func TestRustdocFailureOutputPassesThroughWhenNothingLooksLikeADiagnostic(t *testing.T) {
	// A toolchain failure, a killed process, or a linker error has no diagnostic
	// block. Swallowing it would report an empty reason for a red check.
	if got := rustdocFailureOutput("some linker noise\nno diagnostics here\n"); !strings.Contains(got, "linker noise") {
		t.Fatalf("unrecognized output must pass through whole, got %q", got)
	}
	if got := rustdocFailureOutput("error[E0124]: field is already declared\n"); !strings.Contains(got, "E0124") {
		t.Fatalf("a compile error is still an error block, got %q", got)
	}
}

// An unresolved link with NO `-->` locator is rustdoc's signature for the
// merged-doc-fragment trap, and it's the one failure a reader can't act on: no
// file, no line, and the item it says is missing is usually right there in the
// same file. The check explains it rather than leaving each reader to rediscover
// it (`canonical_root.rs` hit it 12 days after the last one).
const spanlessRustdocOutput = ` Documenting cmdr-fs v0.0.0 (/repo/crates/cmdr-fs)
error: unresolved link to ` + "`collapse_by_volume_id`" + `
   |
   = note: the link appears in this line:
           [` + "`collapse_by_volume_id`" + `] here, because the rule is a pure list transform
            ^^^^^^^^^^^^^^^^^^^^^^
   = note: no item named ` + "`collapse_by_volume_id`" + ` in scope
`

func TestRustdocSpanlessUnresolvedLinkExplainsTheMergedDocTrap(t *testing.T) {
	got := rustdocFailureOutput(spanlessRustdocOutput)
	if !strings.Contains(got, "collapse_by_volume_id") {
		t.Fatalf("the diagnostic itself must survive:\n%s", got)
	}
	if !strings.Contains(got, "hint:") {
		t.Fatalf("a spanless unresolved link must carry a hint:\n%s", got)
	}
	// Naming both halves is what makes it actionable: the reader has to know to
	// look at the `mod` declaration, not at the file the link is written in.
	for _, want := range []string{"mod ", "outer"} {
		if !strings.Contains(got, want) {
			t.Errorf("the hint should name the %q half of the trap:\n%s", want, got)
		}
	}
}

func TestRustdocHintStaysOffDiagnosticsThatCarryALocator(t *testing.T) {
	// A located error tells the reader where to look, so the hint would be noise
	// on every ordinary broken link.
	if got := rustdocFailureOutput(realRustdocOutput); strings.Contains(got, "hint:") {
		t.Errorf("a located diagnostic must not carry the merged-fragment hint:\n%s", got)
	}
}

func TestRustdocRunsInItsOwnBuildDirLocallyAndTheSharedOneInCI(t *testing.T) {
	const wantDir = "CARGO_TARGET_DIR=/repo/target/rustdoc"

	local := rustdocEnv([]string{"PATH=/bin"}, "/repo", "-D rustdoc::bare_urls", false)
	if !slices.Contains(local, wantDir) {
		t.Errorf("a local run needs its own build dir to stay off the shared lock, got %v", local)
	}

	ci := rustdocEnv([]string{"PATH=/bin"}, "/repo", "-D rustdoc::bare_urls", true)
	for _, entry := range ci {
		if strings.HasPrefix(entry, "CARGO_TARGET_DIR=") {
			t.Errorf("CI must keep the shared, cached build dir; the runner is disk-tight. Got %q", entry)
		}
	}

	// The lint contract rides along either way; it IS the check.
	for _, env := range [][]string{local, ci} {
		if !slices.Contains(env, "RUSTDOCFLAGS=-D rustdoc::bare_urls") {
			t.Errorf("the lint flags went missing from %v", env)
		}
	}
}

func TestRustdocDeniedLintsAreRustdocLints(t *testing.T) {
	// A typo'd lint name is silently accepted by rustdoc (it's an unknown-lint
	// warning at most), so the contract would quietly stop being enforced.
	for _, lint := range rustdocDeniedLints {
		if strings.ContainsAny(lint, ":- ") {
			t.Errorf("%q must be the bare lint name; the `rustdoc::` prefix is added by the check", lint)
		}
	}
	if len(rustdocDeniedLints) == 0 {
		t.Fatal("the deny list is the whole contract; an empty one gates nothing")
	}
}

// Shaped like `cargo doc --message-format=json`: diagnostics arrive as JSON on
// stdout, cargo's own lines as text on stderr after them.
const jsonRustdocOutput = `{"reason":"compiler-message","package_id":"path+file:///work/vendor/rustic_core#0.13.0","manifest_path":"/work/vendor/rustic_core/Cargo.toml","message":{"level":"warning","rendered":"warning: lint ` + "`no_mangle_generic_items`" + ` has been removed\n  = note: requested on the command line\n"}}
{"reason":"compiler-artifact","package_id":"path+file:///work/crates/cmdr-fs#0.0.0","manifest_path":"/work/crates/cmdr-fs/Cargo.toml"}
{"reason":"compiler-message","package_id":"path+file:///work/crates/cmdr-restic#0.0.0","manifest_path":"/work/crates/cmdr-restic/Cargo.toml","message":{"level":"error","rendered":"error: unresolved link to ` + "`limit`" + `\n  --> crates/cmdr-restic/src/lib.rs:4:5\n"}}
{"reason":"build-finished","success":false}
 Documenting cmdr-restic v0.0.0 (/work/crates/cmdr-restic)
error: could not document ` + "`cmdr-restic`" + `
`

func TestRustdocOwnOutputDropsOnlyVendoredCratesDiagnostics(t *testing.T) {
	got := rustdocOwnOutput(jsonRustdocOutput, []string{"vendor/rustic_core", "vendor/zip"})
	// A vendored crate is third-party code that cargo can't cap the lints of, the
	// way it caps a registry crate's, so its warnings aren't this lane's to own.
	if strings.Contains(got, "no_mangle_generic_items") {
		t.Fatalf("a vendored crate's diagnostic survived:\n%s", got)
	}
	// Our own diagnostics come back as the text the rest of the lane reads, and
	// cargo's own stderr lines pass through untouched.
	for _, line := range []string{"error: unresolved link to", "--> crates/cmdr-restic/src/lib.rs:4:5", "could not document"} {
		if !strings.Contains(got, line) {
			t.Fatalf("lost %q:\n%s", line, got)
		}
	}
	if strings.Contains(got, `"reason"`) {
		t.Fatalf("a non-diagnostic JSON line leaked into the text:\n%s", got)
	}
	if kept := rustdocDiagnostics(got); !strings.Contains(kept, "unresolved link to") || strings.Contains(kept, "removed") {
		t.Fatalf("the diagnostics filter saw the wrong set:\n%s", kept)
	}
}
