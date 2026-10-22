package main

import (
	"regexp"
	"slices"
	"strings"
)

const (
	oldRepo = "vdavid/cmdr"
	newRepo = "getcmdr/cmdr"

	// keptLocalPath is the keep reason for `…/vdavid/cmdr` that is a folder on disk, not the repo.
	keptLocalPath = "a local path (`~/projects-git/vdavid/cmdr`), not the GitHub repo"
)

// span is a byte range of a file's text that the generic swap leaves alone, and why.
type span struct {
	start, end int
	why        string
}

type swapResult struct {
	swapped int
	// kept counts the mentions of the repo left on the old name, by reason.
	kept map[string]int
}

// swapContent rewrites every mention of the repo `vdavid/cmdr` to `getcmdr/cmdr`, except inside
// `protected` and `keepPatterns` spans. A mention counts when `vdavid/cmdr` isn't the start of a
// longer repo name (`vdavid/cmdr-reports`) and doesn't sit in a path: after a `/`, only
// `github.com/` and `repos/` qualify, so `~/projects-git/vdavid/cmdr` stays.
func swapContent(content string, protected []span) (string, swapResult) {
	res := swapResult{kept: map[string]int{}}
	spans := append(slices.Clone(protected), keepSpans(content)...)

	var b strings.Builder
	last := 0
	for i := 0; ; {
		j := strings.Index(content[i:], oldRepo)
		if j < 0 {
			break
		}
		at := i + j
		i = at + len(oldRepo)
		if !isRepoMention(content, at) {
			continue
		}
		if why, ok := inSpan(spans, at); ok {
			res.kept[why]++
			continue
		}
		if isLocalPath(content, at) {
			res.kept[keptLocalPath]++
			continue
		}
		b.WriteString(content[last:at])
		b.WriteString(newRepo)
		last = i
		res.swapped++
	}
	b.WriteString(content[last:])
	out := b.String()

	for _, l := range literalSwaps {
		n := strings.Count(out, l.from)
		out = strings.ReplaceAll(out, l.from, l.to)
		res.swapped += n
	}
	return out, res
}

// isRepoMention is false when `vdavid/cmdr` at `at` is part of a longer name on either side.
func isRepoMention(content string, at int) bool {
	if end := at + len(oldRepo); end < len(content) && isNameByte(content[end]) {
		return false // vdavid/cmdr-reports, vdavid/cmdr-fsevent-stream
	}
	if at > 0 {
		if c := content[at-1]; isNameByte(c) || c == '.' {
			return false
		}
	}
	return true
}

func isLocalPath(content string, at int) bool {
	if at == 0 || content[at-1] != '/' {
		return false
	}
	before := content[:at]
	return !strings.HasSuffix(before, "github.com/") && !strings.HasSuffix(before, "repos/")
}

func isNameByte(c byte) bool {
	return c == '-' || c == '_' || (c >= '0' && c <= '9') || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func inSpan(spans []span, at int) (string, bool) {
	for _, s := range spans {
		if at >= s.start && at < s.end {
			return s.why, true
		}
	}
	return "", false
}

func keepSpans(content string) []span {
	var out []span
	for _, k := range keepPatterns {
		for _, loc := range k.re.FindAllStringIndex(content, -1) {
			out = append(out, span{start: loc[0], end: loc[1], why: k.why})
		}
	}
	return out
}

type editStatus int

const (
	editApplied editStatus = iota
	editAlreadyApplied
	editAnchorMissing
)

func (s editStatus) String() string {
	switch s {
	case editApplied:
		return "applied"
	case editAlreadyApplied:
		return "already applied"
	default:
		return "ANCHOR MISSING"
	}
}

// applyEdit replaces `e.find` with `e.replace` (with `{cutoff}` filled in), matching any run of
// whitespace as any other, so a formatter rewrapping either text doesn't matter.
func applyEdit(content string, e edit, cutoff string) (string, editStatus) {
	if findRE := flexRE(e.find); findRE.MatchString(content) {
		return findRE.ReplaceAllLiteralString(content, e.replacement(cutoff)), editApplied
	}
	if len(editSpans(content, e, cutoff)) > 0 {
		return content, editAlreadyApplied
	}
	return content, editAnchorMissing
}

// editSpans are where `e`'s replacement sits in `content`, which the generic swap leaves alone.
// Compute them after the file's last edit: an edit earlier in the file shifts every offset.
func editSpans(content string, e edit, cutoff string) []span {
	var spans []span
	for _, loc := range flexRE(e.replacement(cutoff)).FindAllStringIndex(content, -1) {
		spans = append(spans, span{start: loc[0], end: loc[1], why: e.why})
	}
	return spans
}

func (e edit) replacement(cutoff string) string {
	return strings.ReplaceAll(e.replace, "{cutoff}", cutoff)
}

// flexRE matches `s` literally, except that any whitespace run matches any other.
func flexRE(s string) *regexp.Regexp {
	words := strings.Fields(s)
	for i, w := range words {
		words[i] = regexp.QuoteMeta(w)
	}
	return regexp.MustCompile(strings.Join(words, `\s+`))
}

// globMatch matches a repo-relative path against a glob where `*` stays within one path
// segment and `**/` spans any number of them (zero included).
func globMatch(glob, path string) bool {
	var re strings.Builder
	re.WriteString("^")
	for i := 0; i < len(glob); i++ {
		switch {
		case strings.HasPrefix(glob[i:], "**/"):
			re.WriteString("(?:.*/)?")
			i += 2
		case strings.HasPrefix(glob[i:], "**"):
			re.WriteString(".*")
			i++
		case glob[i] == '*':
			re.WriteString("[^/]*")
		case glob[i] == '?':
			re.WriteString("[^/]")
		default:
			re.WriteString(regexp.QuoteMeta(glob[i : i+1]))
		}
	}
	re.WriteString("$")
	return regexp.MustCompile(re.String()).MatchString(path)
}
