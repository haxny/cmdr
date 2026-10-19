package checks

import "strings"

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
