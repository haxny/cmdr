package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
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

// logMacroPattern finds a log macro invocation in masked source: the `log` crate's
// five level macros and `log!` (bare or path-qualified), plus the app's
// `log_error!`. The `\b` keeps `eprintln!` and `debug_assert!` out, and the
// delimiter after the `!` keeps `macro_rules! log_error {` out.
var logMacroPattern = regexp.MustCompile(`\b(?:trace|debug|info|warn|error|log|log_error)\s*!\s*[(\[{]`)

// formatMacroPattern finds the formatting macros whose arguments a log macro's
// argument can nest (judged as part of it), and that turn a marked value into a
// `String` when they sit outside one.
var formatMacroPattern = regexp.MustCompile(`\bformat(?:_args)?\s*!\s*[(\[{]`)

// logWrapperPattern names the `cmdr_fs` log wrappers. An argument built through
// one of them already marks its values for the bundle's span pass.
var logWrapperPattern = regexp.MustCompile(`\b(?:LogPrivate|LogDetail|LoggedText|LogError)\b`)

// markedStringFormatPattern names the wrappers whose `format!` output is a marked
// string. `LoggedText` is left out on purpose: it re-quotes our own log text,
// which already carries its markers (the Flow B auto note).
var markedStringFormatPattern = regexp.MustCompile(`\b(?:LogPrivate|LogDetail|LogError)\b`)

// markedToStringPattern finds a `LogPrivate` / `LogError` constructor call, whose
// `.to_string()` would turn log text into data.
var markedToStringPattern = regexp.MustCompile(`\b(?:LogPrivate\s*::\s*[a-z_]+|LogError)\s*\(`)

// namedArgPattern reads a `name = expr` format argument.
var namedArgPattern = regexp.MustCompile(`^\s*[A-Za-z_][A-Za-z0-9_]*\s*=([^=]|$)`)

// lossyConversionPattern is the conversion that prints a path or OS string bare.
var lossyConversionPattern = regexp.MustCompile(`\.\s*(display|to_string_lossy)\s*\(\s*\)`)

// identityStems are the words that make a name read as a path or a user's
// identity. A name reads as one when its LAST word (after dropping a
// representation suffix like `_str`) is one of these, so `src_path` and
// `volume_name` count while `file_count` and `volume_id` don't.
var identityStems = setOf(
	"path", "dir", "directory", "file", "name", "folder", "root", "dest", "dst", "destination",
	"source", "src", "share", "volume", "label", "host", "server", "user", "parent",
)

// identityCompounds are the one-word spellings of a stem pair (`filename`), also
// matched across the last two words of a name (`mount_point`).
var identityCompounds = setOf(
	"filename", "dirname", "hostname", "username", "basename", "pathname", "filepath", "dirpath",
	"foldername", "sharename", "servername", "volname", "mountpoint", "subdir", "subpath",
	"subfolder", "homedir", "tmpdir", "tempdir", "workdir", "cwd",
)

// identityReprSuffixes say how a value is held, not what it is, so a name ending
// in one is judged by the word before it (`path_str`, `name_lower`).
var identityReprSuffixes = setOf(
	"str", "string", "buf", "ref", "os", "lossy", "display", "raw", "abs", "absolute", "rel",
	"relative", "canon", "canonical", "full", "lower", "lowercase", "norm", "normalized", "owned",
	"utf8", "bytes", "opt", "trimmed", "folded", "nfc", "nfd", "key", "text",
)

// tallyPlurals are the plurals that name a COUNT in a log line (`tally.files`,
// `self.dirs.load(…)`); a collection of paths is spelled `paths`.
var tallyPlurals = setOf("files", "dirs", "folders")

// quantityWords, leading a name, make it a number (`total_dirs`, `num_paths`).
var quantityWords = setOf("total", "num", "n", "max", "min")

// predicateWords anywhere but last make a name a bool (`is_dir`, `source_is_dir`).
var predicateWords = setOf("is", "has", "use", "should", "can", "in", "was", "needs")

// scalarTails are the accessors whose result carries nothing of the value they
// were called on: a length, a flag, a duration, an extension. A chain ending in
// one prints a fact ABOUT a path, not the path.
var scalarTails = setOf(
	"len", "count", "is_empty", "is_some", "is_none", "is_ok", "is_err", "exists", "try_exists",
	"is_dir", "is_file", "is_symlink", "is_absolute", "is_relative", "extension", "elapsed",
	"as_secs", "as_secs_f32", "as_secs_f64", "as_millis", "as_micros", "as_nanos", "kind",
	"raw_os_error", "capacity", "depth", "starts_with", "ends_with", "contains", "eq", "ne",
	"contains_key", "is_ascii",
)

// transparentTails pass their receiver through unchanged, so a chain is judged by
// what comes before them.
var transparentTails = setOf(
	"unwrap", "expect", "as_ref", "as_deref", "clone", "to_owned", "borrow", "copied", "cloned",
	"unwrap_or_default",
)

// rustKeywords never name a value.
var rustKeywords = setOf(
	"as", "async", "await", "break", "const", "continue", "crate", "else", "enum", "false", "fn",
	"for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
	"self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
)

func setOf(words ...string) map[string]bool {
	m := make(map[string]bool, len(words))
	for _, w := range words {
		m[w] = true
	}
	return m
}

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

	var all logPrivateFileFindings
	scanned := 0
	for _, root := range roots {
		rootFindings, rootScanned, scanErr := scanForLogPrivateValues(ctx.RootDir, root)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
		}
		all.sites = append(all.sites, rootFindings.sites...)
		all.conversions = append(all.conversions, rootFindings.conversions...)
		all.orphans = append(all.orphans, rootFindings.orphans...)
		scanned += rootScanned
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
	if len(hardParts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(appendNonEmpty(hardParts, staleMsg), "\n"))
	}

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
	if staleMsg != "" {
		return SuccessWithChanges(okMsg + "; " + staleMsg), nil
	}
	return Success(okMsg), nil
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

// logPrivateFile is one file's source, masked twin, and the indexes the judging
// steps share.
type logPrivateFile struct {
	source     string
	masked     string
	literals   map[int]rustLiteral // keyed by start offset
	lineStarts []int
}

func (f *logPrivateFile) lineOf(offset int) int { return lineForOffset(f.lineStarts, offset) }

// snippet returns the source text of [start, end) on one line, for the report.
func (f *logPrivateFile) snippet(start, end int) string {
	return strings.Join(strings.Fields(f.source[start:end]), " ")
}

// scanRustSourceForLogPrivate scans one file's source: the log macros for
// unmarked values, the rest for marked strings, and the opt-out directives for
// orphans.
func scanRustSourceForLogPrivate(source, relPath string) logPrivateFileFindings {
	masked, literals := maskRustSource(source)
	file := &logPrivateFile{source: source, masked: masked, literals: map[int]rustLiteral{}, lineStarts: lineStartOffsets(source)}
	for _, lit := range literals {
		file.literals[lit.start] = lit
	}
	testRegions := findRustTestRegions(masked)
	lines := strings.Split(source, "\n")
	tracker := newDirectiveTracker(AllowLogPrivateComment, "//")
	for i, line := range lines {
		if !inRegions(testRegions, file.lineStarts[i]) {
			tracker.observe(i+1, line)
		}
	}
	excused := func(line int) bool {
		prev := ""
		if line >= 2 {
			prev = lines[line-2]
		}
		cur := lines[line-1]
		if !strings.Contains(prev, AllowLogPrivateComment) && !strings.Contains(cur, AllowLogPrivateComment) {
			return false
		}
		tracker.markUsed(line, cur, prev)
		return true
	}

	var findings logPrivateFileFindings
	var logSpans [][2]int
	for _, loc := range logMacroPattern.FindAllStringIndex(masked, -1) {
		open := loc[1] - 1
		closeAt := matchDelim(masked, open, masked[open], closingDelim(masked[open]))
		if closeAt < 0 || inRegions(testRegions, loc[0]) || insideSpans(logSpans, loc[0]) {
			continue
		}
		logSpans = append(logSpans, [2]int{open, closeAt})
		for _, s := range file.judgeFormatInvocation(open, closeAt) {
			if excused(s.line) {
				continue
			}
			s.relPath = relPath
			findings.sites = append(findings.sites, s)
		}
	}

	for _, line := range file.markedStringConversions(logSpans, testRegions) {
		if !excused(line) {
			findings.conversions = append(findings.conversions, markedStringSite{
				relPath: relPath, line: line, text: strings.TrimSpace(lines[line-1]),
			})
		}
	}
	findings.orphans = tracker.orphans(relPath)
	return findings
}

func closingDelim(open byte) byte {
	switch open {
	case '[':
		return ']'
	case '{':
		return '}'
	}
	return ')'
}

func insideSpans(spans [][2]int, offset int) bool {
	for _, s := range spans {
		if offset > s[0] && offset < s[1] {
			return true
		}
	}
	return false
}

// judgeFormatInvocation judges the arguments of the formatting macro whose
// delimiters sit at open/closeAt. The format string is its first argument that is
// a bare string literal (after `target: …` or a level); a macro with none (a
// `macro_rules!` forwarding `$($arg)*`) has nothing to judge.
func (f *logPrivateFile) judgeFormatInvocation(open, closeAt int) []logPrivateSite {
	args := splitTopLevelArgs(f.masked, open+1, closeAt)
	fmtIdx := -1
	var fmtLit rustLiteral
	for i, a := range args {
		if lit, ok := f.literals[a[0]]; ok && lit.end == a[1] {
			fmtIdx, fmtLit = i, lit
			break
		}
	}
	if fmtIdx < 0 {
		return nil
	}

	var sites []logPrivateSite
	named := map[string]bool{}
	for _, a := range args[fmtIdx+1:] {
		if m := namedArgPattern.FindStringSubmatchIndex(f.masked[a[0]:a[1]]); m != nil {
			named[strings.TrimSpace(strings.SplitN(f.masked[a[0]:a[1]], "=", 2)[0])] = true
		}
		sites = append(sites, f.judgeArg(a[0], a[1])...)
	}
	sites = append(sites, f.judgeCaptures(fmtLit, named)...)
	return sites
}

// judgeCaptures flags each inline capture (`{path:?}`) of a variable whose name
// reads as a path or identity. `{{` is a literal brace; a capture naming one of
// the macro's `name = …` arguments is judged through that argument instead, and a
// capitalized one is a constant of ours (`{STATE_FILE_PATH}`), never a user value.
func (f *logPrivateFile) judgeCaptures(lit rustLiteral, named map[string]bool) []logPrivateSite {
	var sites []logPrivateSite
	content := f.source[lit.contentStart:lit.contentEnd]
	for i := 0; i < len(content); i++ {
		if content[i] != '{' {
			continue
		}
		if i+1 < len(content) && content[i+1] == '{' {
			i++
			continue
		}
		end := strings.IndexByte(content[i:], '}')
		if end < 0 {
			break
		}
		spec := content[i+1 : i+end]
		name := spec
		if colon := strings.IndexByte(spec, ':'); colon >= 0 {
			name = spec[:colon]
		}
		name = strings.TrimSpace(name)
		if name != "" && !named[name] && !(name[0] >= 'A' && name[0] <= 'Z') && readsAsIdentity(name) {
			at := lit.contentStart + i
			sites = append(sites, logPrivateSite{
				line: f.lineOf(at), what: "capture", text: content[i : i+end+1],
			})
		}
		i += end
	}
	return sites
}

// judgeArg judges one format argument, [start, end) in the file. A wrapped value
// is clean; otherwise a bare conversion, or an identifier that reads as a path or
// identity and isn't reduced to a scalar, makes it one unmarked value. A
// `format!` nested inside is judged as its own invocation, once.
func (f *logPrivateFile) judgeArg(start, end int) []logPrivateSite {
	start, end = trimSpan(f.masked, start, end)
	if start >= end {
		return nil
	}
	exprStart := start
	if m := namedArgPattern.FindStringIndex(f.masked[start:end]); m != nil {
		exprStart = start + strings.IndexByte(f.masked[start:end], '=') + 1
	}
	expr := []byte(f.masked[exprStart:end])
	if logWrapperPattern.Match(expr) {
		return nil
	}

	var sites []logPrivateSite
	for _, loc := range formatMacroPattern.FindAllIndex(expr, -1) {
		open := exprStart + loc[1] - 1
		closeAt := matchDelim(f.masked, open, f.masked[open], closingDelim(f.masked[open]))
		if closeAt < 0 || closeAt >= end {
			continue
		}
		sites = append(sites, f.judgeFormatInvocation(open, closeAt)...)
		blankRun(expr, loc[0], closeAt+1-exprStart)
	}

	line := f.lineOf(start)
	text := f.snippet(start, end)
	if m := lossyConversionPattern.FindSubmatch(expr); m != nil {
		return append(sites, logPrivateSite{line: line, what: string(m[1]), text: text})
	}
	if scalarTails[lastChainCall(string(expr))] {
		return sites
	}
	if ident := firstIdentityIdent(string(expr)); ident != "" {
		sites = append(sites, logPrivateSite{line: line, what: "name: " + ident, text: text})
	}
	return sites
}

func trimSpan(s string, start, end int) (int, int) {
	for start < end && isRustSpaceByte(s[start]) {
		start++
	}
	for end > start && isRustSpaceByte(s[end-1]) {
		end--
	}
	return start, end
}

// splitTopLevelArgs splits masked[from:to) at the commas that sit outside every
// bracket, including a turbofish's `<…>` (`collect::<HashMap<_, _>>()`).
func splitTopLevelArgs(masked string, from, to int) [][2]int {
	var args [][2]int
	depth, angle := 0, 0
	argStart := from
	for i := from; i < to; i++ {
		c := masked[i]
		switch {
		case c == '(' || c == '[' || c == '{':
			depth++
		case c == ')' || c == ']' || c == '}':
			depth--
		case c == '<' && (angle > 0 || strings.HasSuffix(strings.TrimRight(masked[from:i], " \t\n"), "::")):
			angle++
		case c == '>' && angle > 0 && masked[i-1] != '-':
			angle--
		case c == ',' && depth == 0 && angle == 0:
			args = append(args, [2]int{argStart, i})
			argStart = i + 1
		}
	}
	if s, e := trimSpan(masked, argStart, to); s < e {
		args = append(args, [2]int{argStart, to})
	}
	for i, a := range args {
		s, e := trimSpan(masked, a[0], a[1])
		args[i] = [2]int{s, e}
	}
	return args
}

// lastChainCall returns the name of the last method or field an expression's
// top-level chain ends on, skipping the ones that pass their receiver through
// (`.unwrap()`, `.clone()`), or "" when the expression doesn't end in one.
func lastChainCall(expr string) string {
	names := chainNamesBackward(expr)
	for _, n := range names {
		if !transparentTails[n] {
			return n
		}
	}
	return ""
}

// chainNamesBackward reads an expression's trailing `.name` / `.name(…)` segments
// from the end, innermost-last, stopping at the first thing that isn't one.
func chainNamesBackward(expr string) []string {
	var names []string
	i := len(strings.TrimRight(expr, " \t\n?"))
	for i > 0 {
		j := i
		if expr[j-1] == ')' {
			open := matchOpenBackward(expr, j-1)
			if open < 0 {
				return names
			}
			j = open
			// A turbofish between the name and its call: `collect::<Vec<_>>()`.
			if k := strings.TrimRight(expr[:j], " \t\n"); strings.HasSuffix(k, ">") {
				if lt := strings.LastIndex(k, "::<"); lt >= 0 {
					j = lt
				}
			}
		}
		nameEnd := j
		for j > 0 && isRustIdentByte(expr[j-1]) {
			j--
		}
		if j == nameEnd {
			return names
		}
		dot := strings.TrimRight(expr[:j], " \t\n?")
		if !strings.HasSuffix(dot, ".") {
			return names
		}
		names = append(names, expr[j:nameEnd])
		i = len(dot) - 1
		i = len(strings.TrimRight(expr[:i], " \t\n?"))
	}
	return names
}

func matchOpenBackward(s string, closeAt int) int {
	open := map[byte]byte{')': '(', ']': '[', '}': '{'}[s[closeAt]]
	depth := 0
	for i := closeAt; i >= 0; i-- {
		switch s[i] {
		case s[closeAt]:
			depth++
		case open:
			depth--
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}

// firstIdentityIdent returns the first identifier in a masked expression that
// reads as a path or identity and whose own chain isn't reduced to a scalar
// (`entry.name.len()`), or "". Types and variants (capitalized), path segments
// (`path::Path`), macro and free-function names, and keywords never count;
// fields and method names do (`entry.path()` returns a path).
func firstIdentityIdent(expr string) string {
	for i := 0; i < len(expr); i++ {
		if !isRustIdentByte(expr[i]) || (i > 0 && isRustIdentByte(expr[i-1])) {
			continue
		}
		j := i
		for j < len(expr) && isRustIdentByte(expr[j]) {
			j++
		}
		ident := expr[i:j]
		next := strings.TrimLeft(expr[j:], " \t\n")
		prev := strings.TrimRight(expr[:i], " \t\n")
		isMethod := strings.HasSuffix(prev, ".")
		switch {
		case ident[0] >= '0' && ident[0] <= '9', ident[0] >= 'A' && ident[0] <= 'Z', rustKeywords[ident]:
		case strings.HasPrefix(next, "::"), strings.HasPrefix(next, "!"), strings.HasSuffix(prev, "'"):
		case strings.HasPrefix(next, "(") && !isMethod:
		case readsAsIdentity(ident) && !scalarTails[firstChainName(expr[j:])]:
			return ident
		}
		i = j - 1
	}
	return ""
}

// firstChainName returns the name of the last non-transparent segment of the
// chain that starts right after an identifier (`(…)` of its own call first, then
// `.a().b`), or "" when nothing follows it.
func firstChainName(rest string) string {
	i := 0
	skipSpace := func() {
		for i < len(rest) && (isRustSpaceByte(rest[i]) || rest[i] == '?') {
			i++
		}
	}
	skipCall := func() bool {
		skipSpace()
		if strings.HasPrefix(rest[i:], "::<") {
			depth := 0
			for ; i < len(rest); i++ {
				if rest[i] == '<' {
					depth++
				} else if rest[i] == '>' {
					depth--
					if depth == 0 {
						i++
						break
					}
				}
			}
			skipSpace()
		}
		if i < len(rest) && (rest[i] == '(' || rest[i] == '[') {
			closeAt := matchDelim(rest, i, rest[i], closingDelim(rest[i]))
			if closeAt < 0 {
				return false
			}
			i = closeAt + 1
		}
		return true
	}
	if !skipCall() {
		return ""
	}
	last := ""
	for {
		skipSpace()
		if i >= len(rest) || rest[i] != '.' {
			return last
		}
		i++
		skipSpace()
		start := i
		for i < len(rest) && isRustIdentByte(rest[i]) {
			i++
		}
		if i == start {
			return last
		}
		if name := rest[start:i]; !transparentTails[name] {
			last = name
		}
		if !skipCall() {
			return last
		}
	}
}

// readsAsIdentity reports whether a Rust identifier names a path or a user's
// identity: its last word, after any representation suffix, is an identity stem
// (`src_path`, `volume_name`, `path_str`, `hosts`), or its last two words spell a
// compound (`mount_point`). `file_count` and `volume_id` end on a word that
// isn't one, so they don't, and neither do a tally (`files`, `total_dirs`) or a
// flag (`is_dir`).
func readsAsIdentity(name string) bool {
	var words []string
	for _, w := range strings.Split(strings.ToLower(name), "_") {
		if w != "" {
			words = append(words, w)
		}
	}
	for len(words) > 1 && identityReprSuffixes[words[len(words)-1]] {
		words = words[:len(words)-1]
	}
	if len(words) == 0 || (len(words) > 1 && quantityWords[words[0]]) {
		return false
	}
	for _, w := range words[:len(words)-1] {
		if predicateWords[w] {
			return false
		}
	}
	last := words[len(words)-1]
	if tallyPlurals[last] {
		return false
	}
	if isIdentityWord(last) {
		return true
	}
	return len(words) >= 2 && identityCompounds[words[len(words)-2]+last]
}

func isIdentityWord(w string) bool {
	if identityStems[w] || identityCompounds[w] {
		return true
	}
	if strings.HasSuffix(w, "ies") && identityStems[strings.TrimSuffix(w, "ies")+"y"] {
		return true
	}
	singular := strings.TrimSuffix(w, "s")
	return singular != w && (identityStems[singular] || identityCompounds[singular])
}

// markedStringConversions returns the lines outside every log macro (and every
// test region) where a marked value becomes a `String`: a `format!` whose
// arguments hold a `LogPrivate` or `LogDetail`, or a `LogPrivate::…(…)` call
// followed by `.to_string()`.
func (f *logPrivateFile) markedStringConversions(logSpans, testRegions [][2]int) []int {
	var lines []int
	outside := func(offset int) bool {
		return !insideSpans(logSpans, offset) && !inRegions(testRegions, offset)
	}
	for _, loc := range formatMacroPattern.FindAllStringIndex(f.masked, -1) {
		if strings.HasPrefix(f.masked[loc[0]:], "format_args") {
			continue // a borrowed `Arguments`, never a `String`
		}
		open := loc[1] - 1
		closeAt := matchDelim(f.masked, open, f.masked[open], closingDelim(f.masked[open]))
		if closeAt < 0 || !outside(loc[0]) {
			continue
		}
		if markedStringFormatPattern.MatchString(f.masked[open:closeAt]) {
			lines = append(lines, f.lineOf(loc[0]))
		}
	}
	for _, loc := range markedToStringPattern.FindAllStringIndex(f.masked, -1) {
		open := loc[1] - 1
		closeAt := matchDelim(f.masked, open, '(', ')')
		if closeAt < 0 || !outside(loc[0]) {
			continue
		}
		after := strings.TrimLeft(f.masked[closeAt+1:], " \t\n")
		if strings.HasPrefix(after, ".to_string()") {
			lines = append(lines, f.lineOf(loc[0]))
		}
	}
	return lines
}
