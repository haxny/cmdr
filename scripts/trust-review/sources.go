package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
)

const (
	sourceMapPath = "apps/website/src/lib/trust-sources.json"
	trustDataPath = "apps/website/src/lib/trust.ts"
)

type sourceMap struct {
	Sections []section `json:"sections"`
}

// section is one `<h2 id>` of a trust page and the repo paths its claims rest on.
type section struct {
	// Page is the URL path, for example `/trust` or `/trust/development`.
	Page   string `json:"page"`
	Anchor string `json:"anchor"`
	// Paths are repo-relative patterns: `path.Match` per segment, plus `**` for any number of
	// segments (zero included).
	Paths []string `json:"paths"`
}

func loadSourceMap(root string) (sourceMap, error) {
	var m sourceMap
	data, err := os.ReadFile(filepath.Join(root, sourceMapPath))
	if err != nil {
		return m, err
	}
	if err := json.Unmarshal(data, &m); err != nil {
		return m, fmt.Errorf("%s: %w", sourceMapPath, err)
	}
	return m, nil
}

// matching returns the changed files that can make the section's claims wrong, in input order. A
// doc or test file counts only when a pattern names it exactly: docs and tests change on most
// commits without changing what the app does, and counting them flagged most of the page on
// every release.
func (s section) matching(files []string) []string {
	var out []string
	for _, f := range files {
		if isDocOrTest(f) {
			if slices.Contains(s.Paths, f) {
				out = append(out, f)
			}
			continue
		}
		if slices.ContainsFunc(s.Paths, func(p string) bool { return matchPattern(p, f) }) {
			out = append(out, f)
		}
	}
	return out
}

var docOrTestPatterns = []string{
	"**/CLAUDE.md", "**/DETAILS.md",
	"**/tests/**", "**/test/**", "**/testdata/**",
	"**/*_test.rs", "**/*_tests.rs", "**/tests.rs", "**/*_test.go", "**/*.test.ts", "**/*.spec.ts", "**/*.a11y.test.ts",
}

func isDocOrTest(file string) bool {
	return slices.ContainsFunc(docOrTestPatterns, func(p string) bool { return matchPattern(p, file) })
}

// pageFile maps a trust URL to its Astro source: `/trust` to `trust.astro`, `/trust/x` to `trust/x.astro`.
func pageFile(page string) string {
	return "apps/website/src/pages" + page + ".astro"
}

var h2IDRE = regexp.MustCompile(`<h2[^>]*\bid="([^"{}]+)"`)

// headingIDs lists a page's static `<h2 id>` anchors. A heading whose id is computed (`id={...}`)
// isn't matched, so a page built from data needs its section anchors written out.
func headingIDs(src string) []string {
	var ids []string
	for _, m := range h2IDRE.FindAllStringSubmatch(src, -1) {
		ids = append(ids, m[1])
	}
	return ids
}

// validate reports every drift between the map and the pages: a mapped page or anchor that doesn't
// exist, a page heading with no entry, a duplicate entry, and a pattern no tracked file matches.
func (m sourceMap) validate(root string, tracked []string) ([]string, error) {
	var problems []string
	ids := map[string][]string{}
	seen := map[string]bool{}
	for _, s := range m.Sections {
		key := s.Page + "#" + s.Anchor
		if seen[key] {
			problems = append(problems, key+": listed twice")
		}
		seen[key] = true
		if _, ok := ids[s.Page]; !ok {
			src, err := os.ReadFile(filepath.Join(root, pageFile(s.Page)))
			if err != nil {
				problems = append(problems, fmt.Sprintf("%s: no page at %s", key, pageFile(s.Page)))
				ids[s.Page] = nil
				continue
			}
			ids[s.Page] = headingIDs(string(src))
		}
		if !slices.Contains(ids[s.Page], s.Anchor) {
			problems = append(problems, key+": no such <h2 id> on the page")
		}
		if len(s.Paths) == 0 {
			problems = append(problems, key+": no paths")
		}
		for _, p := range s.Paths {
			if !slices.ContainsFunc(tracked, func(f string) bool { return matchPattern(p, f) }) {
				problems = append(problems, fmt.Sprintf("%s: %q matches no tracked file", key, p))
			}
		}
	}
	for page, pageIDs := range ids {
		for _, id := range pageIDs {
			if !seen[page+"#"+id] {
				problems = append(problems, fmt.Sprintf("%s#%s: heading has no entry; add the paths its claims rest on", page, id))
			}
		}
	}
	slices.Sort(problems)
	return problems, nil
}

// matchPattern matches a repo-relative file against a pattern of `/`-separated segments, where
// `**` spans any number of segments and every other segment follows `path.Match`.
func matchPattern(pattern, file string) bool {
	return matchSegments(strings.Split(pattern, "/"), strings.Split(file, "/"))
}

func matchSegments(pat, segs []string) bool {
	for len(pat) > 0 {
		if pat[0] == "**" {
			for i := 0; i <= len(segs); i++ {
				if matchSegments(pat[1:], segs[i:]) {
					return true
				}
			}
			return false
		}
		if len(segs) == 0 {
			return false
		}
		if ok, err := path.Match(pat[0], segs[0]); err != nil || !ok {
			return false
		}
		pat, segs = pat[1:], segs[1:]
	}
	return len(segs) == 0
}

type verifiedAgainst struct{ version, date string }

var verifiedRE = regexp.MustCompile(`export const verifiedAgainst = \{ version: '(\d+\.\d+\.\d+)', date: '(\d{4}-\d{2}-\d{2})' \}`)

func readVerifiedAgainst(root string) (verifiedAgainst, error) {
	src, err := os.ReadFile(filepath.Join(root, trustDataPath))
	if err != nil {
		return verifiedAgainst{}, err
	}
	return parseVerifiedAgainst(string(src))
}

func parseVerifiedAgainst(src string) (verifiedAgainst, error) {
	m := verifiedRE.FindStringSubmatch(src)
	if m == nil {
		return verifiedAgainst{}, fmt.Errorf("%s: no `export const verifiedAgainst = { version: 'X.Y.Z', date: 'YYYY-MM-DD' }` line", trustDataPath)
	}
	return verifiedAgainst{version: m[1], date: m[2]}, nil
}
