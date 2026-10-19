package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// AllowLogPrivateComment opts one line out of the log-private-values check, for a
// value that only LOOKS like the user's (a setting's name, a thread's name, a
// fixed path of our own). Place it on the line above the flagged line, or as a
// trailing comment on it, with a reason.
//
//	// allowed-log-private: the setting's key, not a user value
//	log::debug!("setting {} changed", name);
const AllowLogPrivateComment = "// allowed-log-private:"

// logPrivateAllowlistRelPath is where the per-file budget of unmarked values
// lives, next to the check source files.
const logPrivateAllowlistRelPath = "scripts/check/checks/log-private-values-allowlist.json"

// logPrivateSite is one unmarked value inside a log macro.
type logPrivateSite struct {
	relPath string
	line    int
	what    string // why it reads as a user value: `display`, `name: src_path`, `capture`
	text    string
}

// markedStringSite is a marked value turned into a `String` outside a log macro.
type markedStringSite struct {
	relPath string
	line    int
	text    string
}

type logPrivateFileFindings struct {
	sites       []logPrivateSite
	conversions []markedStringSite
	orphans     []orphanDirective
}

// logPrivateAllowance is one file's budget of unmarked values and why it has one.
type logPrivateAllowance struct {
	Count  int    `json:"count"`
	Reason string `json:"reason"`
}

// logPrivateAllowlist is the on-disk shape of log-private-values-allowlist.json:
// per file, the number of unmarked values it may still hold. A count, not a list
// of lines, so an edit above a site doesn't churn the file.
type logPrivateAllowlist struct {
	Comment string                         `json:"$comment,omitempty"`
	Files   map[string]logPrivateAllowance `json:"files"`
}

// RunLogPrivateValues holds every log macro in non-test Rust code to the marker
// convention (`crates/cmdr-fs/src/log_private.rs`): a path, name, volume, host,
// or account goes into a log line through `LogPrivate` (or `LogDetail` /
// `LoggedText` for text), so the error-report bundle hashes the exact value
// instead of guessing where it ends. It flags, inside a log macro:
//
//   - `.display()` and `.to_string_lossy()`, which print a path bare;
//   - an argument or inline capture whose name reads as a path or identity
//     (`readsAsIdentity`), unless its chain ends in a scalar (`paths.len()`).
//
// Those are budgeted per file by the allowlist, which only shrinks: the budget IS
// the migration's progress bar. Outside log macros it fails outright on a
// `LogPrivate` turned into a `String` (`.to_string()`, `format!`), since a marked
// string must never become a path, a stored value, or UI copy.
func RunLogPrivateValues(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-log-private-values")
	if err != nil {
		return CheckResult{}, err
	}
	all, scanned, err := scanLogPrivateRoots(ctx.RootDir, roots)
	if err != nil {
		return CheckResult{}, err
	}

	allowlist := loadLogPrivateAllowlist(ctx.RootDir)
	counts := map[string]int{}
	for _, s := range all.sites {
		counts[s.relPath]++
	}
	staleChanges := shrinkwrapLogPrivateAllowlist(&allowlist, counts)
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(filepath.Join(ctx.RootDir, logPrivateAllowlistRelPath), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, logPrivateAllowlistRelPath)
	}
	staleMsg := formatLogPrivateStaleness(staleChanges, ctx.CI)

	if hardParts := logPrivateHardFailures(all, counts, allowlist); len(hardParts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(appendNonEmpty(hardParts, staleMsg), "\n"))
	}

	okMsg := formatLogPrivateSuccess(scanned, allowlist)
	if staleMsg != "" {
		return SuccessWithChanges(okMsg + "; " + staleMsg), nil
	}
	return Success(okMsg), nil
}

func scanLogPrivateRoots(rootDir string, roots []string) (logPrivateFileFindings, int, error) {
	var all logPrivateFileFindings
	scanned := 0
	for _, root := range roots {
		rootFindings, rootScanned, err := scanForLogPrivateValues(rootDir, root)
		if err != nil {
			return logPrivateFileFindings{}, 0, fmt.Errorf("failed to scan Rust files: %w", err)
		}
		all.sites = append(all.sites, rootFindings.sites...)
		all.conversions = append(all.conversions, rootFindings.conversions...)
		all.orphans = append(all.orphans, rootFindings.orphans...)
		scanned += rootScanned
	}
	return all, scanned, nil
}

// logPrivateHardFailures returns the report sections that fail the check: files
// over budget, marked strings, reasonless entries, and orphan directives.
func logPrivateHardFailures(all logPrivateFileFindings, counts map[string]int, allowlist logPrivateAllowlist) []string {
	var overBudget []logPrivateSite
	for _, s := range all.sites {
		if counts[s.relPath] > allowlist.Files[s.relPath].Count {
			overBudget = append(overBudget, s)
		}
	}

	var hardParts []string
	if len(overBudget) > 0 {
		hardParts = append(hardParts, formatLogPrivateOverBudget(overBudget, allowlist))
	}
	if len(all.conversions) > 0 {
		hardParts = append(hardParts, formatMarkedStringSites(all.conversions))
	}
	if reasonless := reasonlessLogPrivateEntries(allowlist); len(reasonless) > 0 {
		hardParts = append(hardParts, fmt.Sprintf(
			"%d %s in %s %s no reason; every entry needs one:\n  - %s",
			len(reasonless), Pluralize(len(reasonless), "entry", "entries"), logPrivateAllowlistRelPath,
			Pluralize(len(reasonless), "has", "have"), strings.Join(reasonless, "\n  - ")))
	}
	if len(all.orphans) > 0 {
		hardParts = append(hardParts, formatOrphanDirectives(AllowLogPrivateComment, all.orphans))
	}
	return hardParts
}

func formatLogPrivateSuccess(scanned int, allowlist logPrivateAllowlist) string {
	allowlisted := 0
	for _, entry := range allowlist.Files {
		allowlisted += entry.Count
	}
	okMsg := fmt.Sprintf("%d Rust %s scanned, no log macro adds an unmarked user value",
		scanned, Pluralize(scanned, "file", "files"))
	if allowlisted > 0 {
		okMsg += fmt.Sprintf(" (%s unmarked %s allowlisted across %s %s)",
			FormatThousands(allowlisted), Pluralize(allowlisted, "value", "values"),
			FormatThousands(len(allowlist.Files)), Pluralize(len(allowlist.Files), "file", "files"))
	}
	return okMsg
}

// formatLogPrivateOverBudget lists each over-budget file with every unmarked value
// it holds: the budget is per file, so which site to convert is the reader's pick.
func formatLogPrivateOverBudget(sites []logPrivateSite, allowlist logPrivateAllowlist) string {
	byFile := map[string][]logPrivateSite{}
	for _, s := range sites {
		byFile[s.relPath] = append(byFile[s.relPath], s)
	}
	var sb strings.Builder
	for _, relPath := range sortedKeys(byFile) {
		fileSites := byFile[relPath]
		sort.SliceStable(fileSites, func(i, j int) bool { return fileSites[i].line < fileSites[j].line })
		sb.WriteString(fmt.Sprintf("  - %s: %d unmarked %s (allowlist: %d)\n",
			relPath, len(fileSites), Pluralize(len(fileSites), "value", "values"), allowlist.Files[relPath].Count))
		for _, s := range fileSites {
			sb.WriteString(fmt.Sprintf("      %s:%d [%s] %s\n", relPath, s.line, s.what, s.text))
		}
	}
	return fmt.Sprintf(
		"%d %s over budget for unmarked user values in log macros (wrap each value in "+
			"`cmdr_fs::log_private::LogPrivate::<kind>(…)`, keeping the site's `key=` and format spec, or "+
			"`LogDetail(text).marking(&[…])` for foreign text; `%s <reason>` opts out a value that isn't the user's):\n%s",
		len(byFile), Pluralize(len(byFile), "file", "files"), AllowLogPrivateComment,
		strings.TrimRight(sb.String(), "\n"))
}

func formatMarkedStringSites(sites []markedStringSite) string {
	sort.Slice(sites, func(i, j int) bool {
		if sites[i].relPath == sites[j].relPath {
			return sites[i].line < sites[j].line
		}
		return sites[i].relPath < sites[j].relPath
	})
	var sb strings.Builder
	for _, s := range sites {
		sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", s.relPath, s.line, s.text))
	}
	return fmt.Sprintf(
		"found %d marked %s turned into a `String` outside a log macro (a marked string is log text, never "+
			"a path, a stored value, or UI copy: pass the wrapper to the log macro itself):\n%s",
		len(sites), Pluralize(len(sites), "value", "values"), strings.TrimRight(sb.String(), "\n"))
}

func formatLogPrivateStaleness(changes []string, ci bool) string {
	if len(changes) == 0 {
		return ""
	}
	verb := "Shrink-wrapped allowlist"
	if ci {
		verb = "Stale allowlist entries (a local run shrink-wraps them)"
	}
	return fmt.Sprintf("%s:\n  - %s", verb, strings.Join(changes, "\n  - "))
}

// loadLogPrivateAllowlist reads the budget. A missing or unparsable file yields an
// empty allowlist, so every unmarked value gets reported.
func loadLogPrivateAllowlist(rootDir string) logPrivateAllowlist {
	var list logPrivateAllowlist
	data, err := os.ReadFile(filepath.Join(rootDir, logPrivateAllowlistRelPath))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return logPrivateAllowlist{}
	}
	return list
}

// shrinkwrapLogPrivateAllowlist drops entries whose file holds no unmarked value
// any more and ratchets the rest down to the current count, keeping each reason.
func shrinkwrapLogPrivateAllowlist(list *logPrivateAllowlist, counts map[string]int) []string {
	var changes []string
	for _, path := range sortedKeys(list.Files) {
		entry := list.Files[path]
		current := counts[path]
		switch {
		case current == 0:
			delete(list.Files, path)
			changes = append(changes, fmt.Sprintf("removed %s (every log value marked)", path))
		case current < entry.Count:
			changes = append(changes, fmt.Sprintf("ratcheted %s: %d → %d", path, entry.Count, current))
			entry.Count = current
			list.Files[path] = entry
		}
	}
	return changes
}

func reasonlessLogPrivateEntries(list logPrivateAllowlist) []string {
	var out []string
	for _, path := range sortedKeys(list.Files) {
		if strings.TrimSpace(list.Files[path].Reason) == "" {
			out = append(out, path)
		}
	}
	return out
}

func scanForLogPrivateValues(rootDir, srcDir string) (logPrivateFileFindings, int, error) {
	var all logPrivateFileFindings
	scanned := 0
	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}
		relPath = filepath.ToSlash(relPath)
		// Dedicated test files never reach a bundle (isRustTestPath lives with the
		// test-sleep check, in the same package).
		if isRustTestPath(relPath, d.Name()) {
			return nil
		}
		scanned++
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		findings := scanRustSourceForLogPrivate(string(data), relPath)
		all.sites = append(all.sites, findings.sites...)
		all.conversions = append(all.conversions, findings.conversions...)
		all.orphans = append(all.orphans, findings.orphans...)
		return nil
	})
	return all, scanned, err
}
