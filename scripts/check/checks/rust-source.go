package checks

import (
	"strings"
	"unicode/utf8"
)

// The Rust scanners that read code across lines (rather than one line at a time)
// share this file's lexing: a masked copy of the source with every comment and
// literal body blanked, so brackets, commas, and keywords in prose or in a string
// can't derail a parser, while byte offsets and newlines stay where they were.

// rustLiteral is one string literal in a Rust source. start/end cover the whole
// literal, prefix and quotes included (`r#"…"#`, `b"…"`); the content span is
// what sits between the quotes, still escaped.
type rustLiteral struct {
	start, end               int
	contentStart, contentEnd int
	raw                      bool
}

// maskRustSource blanks every comment and the body of every string and char
// literal, keeping each literal's prefix and quotes, and returns the string
// literals it found in source order. Keeping the quotes lets a parser see that an
// argument IS a literal; blanking the body keeps a `(` or `,` inside it from
// counting as code.
func maskRustSource(source string) (string, []rustLiteral) {
	out := []byte(source)
	var literals []rustLiteral
	for i := 0; i < len(out); i++ {
		switch {
		case hasPairAt(out, i, '/', '/'):
			end := indexOfByteFrom(out, i, '\n')
			if end < 0 {
				end = len(out)
			}
			blankRun(out, i, end)
			i = end - 1
		case hasPairAt(out, i, '/', '*'):
			end := endOfBlockComment(out, i)
			blankRun(out, i, end)
			i = end - 1
		case out[i] == '"' || isRawStringStart(out, i):
			lit := scanRustString(out, i)
			blankRun(out, lit.contentStart, lit.contentEnd)
			literals = append(literals, lit)
			i = lit.end - 1
		case out[i] == '\'':
			if end := endOfCharLiteral(out, i); end > 0 {
				blankRun(out, i+1, end-1)
				i = end - 1
			}
		}
	}
	return string(out), literals
}

// maskRustNonCode replaces every comment and string/char literal with spaces,
// quotes included, keeping byte offsets and newlines intact. Everything
// downstream can then match on real code without re-deriving "is this inside a
// comment".
func maskRustNonCode(source string) string {
	masked, literals := maskRustSource(source)
	out := []byte(masked)
	for _, lit := range literals {
		blankRun(out, lit.start, lit.end)
	}
	return string(out)
}

// isRawStringStart reports whether a raw string (`r"…"`, `r#"…"#`, `br"…"`)
// opens at i. The `r` must start a token, so an identifier ending in `r` followed
// by a string (not valid Rust anyway) isn't read as one.
func isRawStringStart(out []byte, i int) bool {
	if out[i] != 'r' {
		return false
	}
	if i > 0 && isRustIdentByte(out[i-1]) && !(out[i-1] == 'b' && (i < 2 || !isRustIdentByte(out[i-2]))) {
		return false
	}
	j := i + 1
	for j < len(out) && out[j] == '#' {
		j++
	}
	return j < len(out) && out[j] == '"'
}

// scanRustString reads the string literal at i: a plain one at its `"`, or a raw
// one at its `r`. A `b` / `c` prefix glued to the front joins the literal's span.
func scanRustString(out []byte, i int) rustLiteral {
	start := i
	if start > 0 && (out[start-1] == 'b' || out[start-1] == 'c') && (start < 2 || !isRustIdentByte(out[start-2])) {
		start--
	}
	if out[i] == '"' {
		for j := i + 1; j < len(out); j++ {
			switch out[j] {
			case '\\':
				j++
			case '"':
				return rustLiteral{start: start, end: j + 1, contentStart: i + 1, contentEnd: j}
			}
		}
		return rustLiteral{start: start, end: len(out), contentStart: i + 1, contentEnd: len(out)}
	}
	hashes := 0
	j := i + 1
	for out[j] == '#' {
		hashes++
		j++
	}
	closer := "\"" + strings.Repeat("#", hashes)
	contentStart := j + 1
	if at := strings.Index(string(out[contentStart:]), closer); at >= 0 {
		contentEnd := contentStart + at
		return rustLiteral{start: start, end: contentEnd + len(closer), contentStart: contentStart, contentEnd: contentEnd, raw: true}
	}
	return rustLiteral{start: start, end: len(out), contentStart: contentStart, contentEnd: len(out), raw: true}
}

// endOfCharLiteral returns the exclusive end of the char literal opening at i, or
// -1 for a lifetime (`&'a str`), which never closes within one character.
func endOfCharLiteral(out []byte, i int) int {
	if i+1 >= len(out) {
		return -1
	}
	if out[i+1] == '\\' {
		// `'\n'`, `'\''`, `'\u{1F600}'`: the escape runs to the next quote.
		for j := i + 3; j < len(out) && j < i+13; j++ {
			if out[j] == '\'' {
				return j + 1
			}
		}
		return -1
	}
	_, size := utf8.DecodeRune(out[i+1:])
	if close := i + 1 + size; close < len(out) && out[close] == '\'' {
		return close + 1
	}
	return -1
}

// blankRun replaces [from, to) with spaces, leaving newlines in place so byte
// offsets and line numbers both survive.
func blankRun(out []byte, from, to int) {
	for i := from; i < to && i < len(out); i++ {
		if out[i] != '\n' {
			out[i] = ' '
		}
	}
}

func hasPairAt(out []byte, i int, first, second byte) bool {
	return out[i] == first && i+1 < len(out) && out[i+1] == second
}

func indexOfByteFrom(out []byte, from int, b byte) int {
	for i := from; i < len(out); i++ {
		if out[i] == b {
			return i
		}
	}
	return -1
}

// endOfBlockComment handles Rust's NESTED `/* /* */ */`, which C-style scanning
// would close one level early.
func endOfBlockComment(out []byte, start int) int {
	depth := 1
	for i := start + 2; i < len(out); {
		switch {
		case hasPairAt(out, i, '/', '*'):
			depth++
			i += 2
		case hasPairAt(out, i, '*', '/'):
			depth--
			i += 2
			if depth == 0 {
				return i
			}
		default:
			i++
		}
	}
	return len(out)
}

func lineStartOffsets(source string) []int {
	starts := []int{0}
	for i := 0; i < len(source); i++ {
		if source[i] == '\n' {
			starts = append(starts, i+1)
		}
	}
	return starts
}

// findRustTestRegions returns the byte ranges of test-gated items (`#[cfg(test)]`,
// `#[test]`, `#[tokio::test]`). The item is whatever the attribute decorates: a
// braced one (`mod tests { … }`, a test fn) yields its range, and an out-of-line
// one (`#[cfg(test)] mod tests;`, a gated `use`) yields nothing, since its code
// lives in another file.
func findRustTestRegions(masked string) [][2]int {
	var regions [][2]int
	for _, marker := range []string{"#[cfg(test)]", "#[test]", "#[tokio::test"} {
		search := 0
		for {
			idx := strings.Index(masked[search:], marker)
			if idx < 0 {
				break
			}
			start := search + idx
			search = start + len(marker)
			open := testItemBodyBrace(masked, search)
			if open < 0 {
				continue
			}
			if end := matchBrace(masked, open); end >= 0 {
				regions = append(regions, [2]int{start, end})
			}
		}
	}
	return regions
}

// testItemBodyBrace returns the `{` that opens the item a test attribute
// decorates, or -1 when the item ends at a `;` first. Brackets are tracked so a
// `;` inside `[u8; 4]` or the rest of `#[tokio::test(…)]` doesn't end the item.
func testItemBodyBrace(masked string, from int) int {
	depth := 0
	for i := from; i < len(masked); i++ {
		switch masked[i] {
		case '(', '[':
			depth++
		case ')', ']':
			if depth > 0 {
				depth--
			}
		case ';':
			if depth == 0 {
				return -1
			}
		case '{':
			return i
		}
	}
	return -1
}

func inRegions(regions [][2]int, offset int) bool {
	for _, r := range regions {
		if offset >= r[0] && offset <= r[1] {
			return true
		}
	}
	return false
}

func isRustIdentByte(b byte) bool {
	return b == '_' || (b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z') || (b >= '0' && b <= '9')
}

func lineForOffset(lineStarts []int, offset int) int {
	lo, hi := 0, len(lineStarts)-1
	for lo < hi {
		mid := (lo + hi + 1) / 2
		if lineStarts[mid] <= offset {
			lo = mid
		} else {
			hi = mid - 1
		}
	}
	return lo + 1
}

func matchBrace(masked string, open int) int {
	return matchDelim(masked, open, '{', '}')
}

func matchDelim(masked string, open int, openCh, closeCh byte) int {
	if open >= len(masked) || masked[open] != openCh {
		return -1
	}
	depth := 0
	for i := open; i < len(masked); i++ {
		switch masked[i] {
		case openCh:
			depth++
		case closeCh:
			depth--
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}
