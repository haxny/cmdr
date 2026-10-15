package main

import (
	"maps"
	"regexp"
	"strings"
)

// The macOS display options `#lib/display-accessibility` mirrors onto
// `<html>` as classes, each a token block in `app.css` that `ParseAppCSS`
// keeps out of the default tables (see `VarTable`) and these layer back on.

// WithIncreaseContrast returns a table with the `html.increase-contrast`
// overrides layered on, mirroring the cascade: the top-level class rule
// (specificity 0,1,1) beats the dark `:root` (0,1,0), so it applies in both
// modes, and the dark-nested class rule then wins in dark mode.
func (v *VarTable) WithIncreaseContrast() *VarTable {
	out := NewVarTable()
	maps.Copy(out.Light, v.Light)
	maps.Copy(out.Light, v.ContrastLight)
	maps.Copy(out.Dark, v.Dark)
	maps.Copy(out.Dark, v.ContrastLight)
	maps.Copy(out.Dark, v.ContrastDark)
	return out
}

// WithDifferentiateWithoutColor returns a table with the
// `html.differentiate-without-color` overrides layered on in both modes (the
// class rule, 0,1,1, beats both `:root` blocks).
func (v *VarTable) WithDifferentiateWithoutColor() *VarTable {
	out := NewVarTable()
	maps.Copy(out.Light, v.Light)
	maps.Copy(out.Light, v.Differentiate)
	maps.Copy(out.Dark, v.Dark)
	maps.Copy(out.Dark, v.Differentiate)
	return out
}

// increaseContrastBlockPattern matches the opening of a token block keyed on
// the Increase contrast class. Only the bare `html.increase-contrast {` form
// carries tokens; a longer selector (`html.increase-contrast .file-pane ...`)
// is a rule the rule walker reads, not a token block.
var increaseContrastBlockPattern = regexp.MustCompile(`html\.increase-contrast\s*\{`)

// differentiateBlockPattern is the same for the Differentiate without color
// class.
var differentiateBlockPattern = regexp.MustCompile(`html\.differentiate-without-color\s*\{`)

// extractIncreaseContrastBlocks copies the custom properties of every bare
// `html.increase-contrast { ... }` block in s into out, and returns s with
// those blocks blanked (same length, so offsets don't shift).
func extractIncreaseContrastBlocks(s string, out map[string]string) string {
	return extractClassTokenBlocks(s, increaseContrastBlockPattern, out)
}

// extractClassTokenBlocks copies the custom properties of every block whose
// opening matches pattern into out, and returns s with those blocks blanked.
func extractClassTokenBlocks(s string, pattern *regexp.Regexp, out map[string]string) string {
	for {
		loc := pattern.FindStringIndex(s)
		if loc == nil {
			return s
		}
		end := matchingBrace(s, loc[1]-1)
		if end < 0 {
			return s
		}
		extractVarDefs(s[loc[1]:end], out)
		s = s[:loc[0]] + strings.Repeat(" ", end+1-loc[0]) + s[end+1:]
	}
}
