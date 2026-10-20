package checks

import "strings"

// identityStems are the words that make a name read as a path or a user's
// identity. A name reads as one when its LAST word (after dropping a
// representation suffix like `_str`) is one of these, so `src_path` and
// `volume_name` count while `file_count` and `volume_id` don't.
var identityStems = setOf(
	"path", "dir", "directory", "file", "name", "folder", "root", "dest", "dst", "destination",
	"source", "src", "share", "volume", "label", "host", "server", "user", "parent",
	// A remote side, a link or copy target, a staging file, a listing entry, an
	// object key or prefix: each IS a path on some backend (an S3 key leaked
	// through `{key}` before these joined).
	"remote", "target", "staging", "entry", "key", "prefix", "bucket", "mount", "link",
	// Where a server lives, and who the user is on it.
	"url", "uri", "addr", "address", "endpoint", "device", "account", "email",
	// A search query holds the user's own words.
	"query",
)

// identityLoneWords read as identity only as a whole name: alone, `from` / `to`
// are the two sides of a copy or rename, while inside a name they're a
// conversion (`to_string`, `from_utf8`) or a verb (`copied_to`).
var identityLoneWords = setOf("from", "to")

// ownIdentityCompounds end on a stem but name something of Cmdr's own.
var ownIdentityCompounds = setOf("logtarget")

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
	"utf8", "bytes", "opt", "trimmed", "folded", "nfc", "nfd", "text",
)

// tallyPlurals are the plurals that name a COUNT in a log line (`tally.files`,
// `self.dirs.load(…)`); a collection of paths is spelled `paths`.
var tallyPlurals = setOf("files", "dirs", "folders", "entries")

// quantityWords, leading a name, make it a number (`total_dirs`, `num_paths`).
var quantityWords = setOf("total", "num", "n", "max", "min")

// predicateWords anywhere but last make a name a bool (`is_dir`, `source_is_dir`).
var predicateWords = setOf("is", "has", "use", "should", "can", "in", "was", "needs", "requires")

// scalarTails are the accessors whose result carries nothing of the value they
// were called on: a length, a flag, a duration, an extension. A chain ending in
// one prints a fact ABOUT a path, not the path.
var scalarTails = setOf(
	"len", "count", "is_empty", "is_some", "is_none", "is_ok", "is_err", "exists", "try_exists",
	"is_dir", "is_file", "is_symlink", "is_absolute", "is_relative", "extension", "elapsed",
	"as_secs", "as_secs_f32", "as_secs_f64", "as_millis", "as_micros", "as_nanos", "kind",
	"raw_os_error", "capacity", "depth", "starts_with", "ends_with", "contains", "eq", "ne",
	"contains_key", "is_ascii", "id", "size", "port",
)

// scalarTailSuffixes make any field or accessor ending in them a scalar
// (`target.volume_id`, `model.adapter_kind`).
var scalarTailSuffixes = []string{"_id", "_kind", "_count", "_len", "_size"}

// isScalarTail reports whether a chain ending in `name` prints a fact about its
// receiver rather than the receiver itself.
func isScalarTail(name string) bool {
	if scalarTails[name] {
		return true
	}
	for _, suffix := range scalarTailSuffixes {
		if strings.HasSuffix(name, suffix) {
			return true
		}
	}
	return false
}

// transparentTails pass their receiver through unchanged, so a chain is judged by
// what comes before them.
var transparentTails = setOf(
	"unwrap", "expect", "as_ref", "as_deref", "clone", "to_owned", "borrow", "copied", "cloned",
	"unwrap_or_default", "unwrap_or",
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
		case readsAsIdentity(ident) && !isScalarTail(firstChainName(expr[j:])):
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
	c := &chainCursor{rest: rest}
	if !c.skipCall() {
		return ""
	}
	last := ""
	for {
		name, ok := c.nextSegment()
		if !ok {
			return last
		}
		if !transparentTails[name] {
			last = name
		}
		if !c.skipCall() {
			return last
		}
	}
}

// chainCursor walks a method chain left to right for firstChainName.
type chainCursor struct {
	rest string
	i    int
}

func (c *chainCursor) skipSpace() {
	for c.i < len(c.rest) && (isRustSpaceByte(c.rest[c.i]) || c.rest[c.i] == '?') {
		c.i++
	}
}

// skipCall steps over a call's turbofish and `(…)` / `[…]`, if any. It reports
// false on an unclosed bracket.
func (c *chainCursor) skipCall() bool {
	c.skipSpace()
	if strings.HasPrefix(c.rest[c.i:], "::<") {
		c.skipTurbofish()
		c.skipSpace()
	}
	if c.i < len(c.rest) && (c.rest[c.i] == '(' || c.rest[c.i] == '[') {
		closeAt := matchDelim(c.rest, c.i, c.rest[c.i], closingDelim(c.rest[c.i]))
		if closeAt < 0 {
			return false
		}
		c.i = closeAt + 1
	}
	return true
}

// skipTurbofish steps past the `<…>` that balances the first `<`, or to the end.
func (c *chainCursor) skipTurbofish() {
	depth := 0
	for ; c.i < len(c.rest); c.i++ {
		switch c.rest[c.i] {
		case '<':
			depth++
		case '>':
			depth--
			if depth == 0 {
				c.i++
				return
			}
		}
	}
}

// nextSegment reads the `.name` that follows, or reports false when none does.
func (c *chainCursor) nextSegment() (string, bool) {
	c.skipSpace()
	if c.i >= len(c.rest) || c.rest[c.i] != '.' {
		return "", false
	}
	c.i++
	c.skipSpace()
	start := c.i
	for c.i < len(c.rest) && isRustIdentByte(c.rest[c.i]) {
		c.i++
	}
	if c.i == start {
		return "", false
	}
	return c.rest[start:c.i], true
}

// readsAsIdentity reports whether a Rust identifier names a path or a user's
// identity: its last word, after any representation suffix, is an identity stem
// (`src_path`, `volume_name`, `path_str`, `hosts`), or its last two words spell a
// compound (`mount_point`). `file_count` and `volume_id` end on a word that
// isn't one, so they don't, and neither do a tally (`files`, `total_dirs`) or a
// flag (`is_dir`).
func readsAsIdentity(name string) bool {
	if identityLoneWords[strings.ToLower(name)] {
		return true
	}
	words := meaningWords(name)
	if len(words) == 0 || (len(words) > 1 && quantityWords[words[0]]) {
		return false
	}
	for _, w := range words[:len(words)-1] {
		if predicateWords[w] {
			return false
		}
	}
	last := words[len(words)-1]
	if tallyPlurals[last] || (len(words) >= 2 && ownIdentityCompounds[words[len(words)-2]+last]) {
		return false
	}
	if isIdentityWord(last) {
		return true
	}
	return len(words) >= 2 && identityCompounds[words[len(words)-2]+last]
}

// meaningWords splits a snake-case name into its lowercase words and drops the
// trailing representation suffixes (`path_str` → `path`), keeping at least one.
func meaningWords(name string) []string {
	var words []string
	for _, w := range strings.Split(strings.ToLower(name), "_") {
		if w != "" {
			words = append(words, w)
		}
	}
	for len(words) > 1 && identityReprSuffixes[words[len(words)-1]] {
		words = words[:len(words)-1]
	}
	return words
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
