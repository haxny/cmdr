package checks

import (
	"regexp"
	"regexp/syntax"
	"strings"
)

// gatedRegexp is a regexp with a `strings.Contains` gate in front: a line that
// lacks the literal every match must contain never reaches the regex engine.
// A per-line scanner wraps its patterns with `gateRegexp` and keeps calling the
// same methods; most source lines fail the gate, which is where the time went
// (`error-string-match` ran 12 regexes on every line of 1,700 files).
//
// The literal is derived from the pattern itself, so it can't drift from it. A
// pattern with no required literal (a top-level alternation, `(?i)`) gets an
// empty gate and runs on every line, exactly as before.
type gatedRegexp struct {
	re      *regexp.Regexp
	literal string
}

func gateRegexp(re *regexp.Regexp) gatedRegexp {
	return gatedRegexp{re: re, literal: requiredLiteral(re.String())}
}

// gateRegexps gates a scanner's whole pattern list at once.
func gateRegexps(res ...*regexp.Regexp) []gatedRegexp {
	gated := make([]gatedRegexp, len(res))
	for i, re := range res {
		gated[i] = gateRegexp(re)
	}
	return gated
}

func (g gatedRegexp) passes(s string) bool {
	return g.literal == "" || strings.Contains(s, g.literal)
}

func (g gatedRegexp) MatchString(s string) bool {
	return g.passes(s) && g.re.MatchString(s)
}

func (g gatedRegexp) FindAllStringSubmatch(s string, n int) [][]string {
	if !g.passes(s) {
		return nil
	}
	return g.re.FindAllStringSubmatch(s, n)
}

func (g gatedRegexp) FindStringIndex(s string) []int {
	if !g.passes(s) {
		return nil
	}
	return g.re.FindStringIndex(s)
}

func (g gatedRegexp) FindAllStringSubmatchIndex(s string, n int) [][]int {
	if !g.passes(s) {
		return nil
	}
	return g.re.FindAllStringSubmatchIndex(s, n)
}

// requiredLiteral returns the longest case-sensitive literal run that every
// match of expr must contain, or "" when there's none to rely on.
func requiredLiteral(expr string) string {
	parsed, err := syntax.Parse(expr, syntax.Perl)
	if err != nil {
		return ""
	}
	return longestRequiredLiteral(parsed.Simplify())
}

func longestRequiredLiteral(re *syntax.Regexp) string {
	if re == nil {
		return ""
	}
	switch re.Op {
	case syntax.OpLiteral:
		if re.Flags&syntax.FoldCase != 0 {
			return ""
		}
		return string(re.Rune)
	case syntax.OpCapture, syntax.OpPlus:
		return longestRequiredLiteral(re.Sub[0])
	case syntax.OpRepeat:
		if re.Min >= 1 {
			return longestRequiredLiteral(re.Sub[0])
		}
	case syntax.OpConcat:
		best, run := "", ""
		keep := func(candidate string) {
			if len(candidate) > len(best) {
				best = candidate
			}
		}
		for _, sub := range re.Sub {
			if sub.Op == syntax.OpLiteral && sub.Flags&syntax.FoldCase == 0 {
				run += string(sub.Rune)
				continue
			}
			keep(run)
			run = ""
			keep(longestRequiredLiteral(sub))
		}
		keep(run)
		return best
	}
	return ""
}
