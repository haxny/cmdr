package main

import (
	"strings"
	"testing"
)

const sampleDifferentiateCSS = `
:root {
  --color-cursor-outline: #eeeeee;
}

html.differentiate-without-color {
  --color-cursor-outline: #111111;
}
`

// TestParseAppCSSKeepsDifferentiateWithoutColorOutOfTheBaseTables mirrors the
// Increase contrast guard: the class block only applies under the class, so it
// must never leak into the default tables, and it must apply in both modes
// when asked for.
func TestParseAppCSSKeepsDifferentiateWithoutColorOutOfTheBaseTables(t *testing.T) {
	vt := ParseAppCSS(sampleDifferentiateCSS)
	if got := vt.Light["color-cursor-outline"]; got != "#eeeeee" {
		t.Errorf("light outline = %q, want the baseline #eeeeee", got)
	}
	dwc := vt.WithDifferentiateWithoutColor()
	for _, mode := range []Mode{ModeLight, ModeDark} {
		if got, _ := dwc.Raw("color-cursor-outline", mode); got != "#111111" {
			t.Errorf("%s outline under the class = %q, want #111111", mode, got)
		}
	}
}

// rowMarkFixture is a white, untinted pane in light mode with one accent, and
// a dark mode the marks also have to clear.
func rowMarkFixture(dashed, dwcDashed string) *VarTable {
	vt := NewVarTable()
	for _, m := range []map[string]string{vt.Light, vt.Dark} {
		m["pane-tint-fg-pct"] = "10%"
		m["color-accent"] = "#d4a006"
		m["color-cursor-inactive"] = "rgba(232, 232, 232, 0.4)"
		m["color-cursor-active"] = "color-mix(in oklch, var(--color-accent), transparent 80%)"
		m["color-cursor-outline"] = "color-mix(in oklch, var(--color-accent), transparent 65%)"
		m["color-cursor-outline-dashed"] = dashed
		m["color-selection-bg"] = "rgba(0, 0, 0, 0.05)"
		m["color-selection-fg-primary"] = "#cc0000"
		m["color-selection-fg-cursor"] = "#a30000"
		m["color-selection-fg-fallback"] = "#1a1a1a"
		m["color-selection-fg"] = "var(--color-selection-fg-primary)"
		m["color-bg-stripe"] = "#fafafa"
	}
	vt.Light["color-bg-primary"] = "#ffffff"
	vt.Dark["color-bg-primary"] = "#ffffff"
	for _, tint := range rowPaneTints[1:] {
		vt.Light[tint.VarName] = "#888888"
	}
	vt.ContrastLight["color-cursor-outline"] = "#5a4300"
	vt.ContrastLight["color-cursor-outline-dashed"] = "#5a4300"
	vt.Differentiate["color-cursor-outline"] = "#5a4300"
	vt.Differentiate["color-cursor-outline-dashed"] = dwcDashed
	return vt
}

func failingWhere(findings []Finding, substr string) []Finding {
	var out []Finding
	for _, f := range findings {
		if !f.IsPassing && strings.Contains(f.Selector, substr) {
			out = append(out, f)
		}
	}
	return out
}

// TestAnalyzeRowMarksGatesTheDashedOutlineUnderDifferentiateWithoutColor:
// under Differentiate without color, solid vs dashed is the focused-pane cue,
// so the dashed hairline has to clear 3:1 on the cursor fill. A faint one fails.
func TestAnalyzeRowMarksGatesTheDashedOutlineUnderDifferentiateWithoutColor(t *testing.T) {
	faint := "color-mix(in oklch, var(--color-accent), transparent 50%)"
	gated, _ := NewAnalyzer(rowMarkFixture(faint, faint)).AnalyzeRowMarks()
	if len(failingWhere(gated, "dashed")) == 0 {
		t.Fatal("a faint dashed outline passed the Differentiate without color gate")
	}
	gated, _ = NewAnalyzer(rowMarkFixture(faint, "#5a4300")).AnalyzeRowMarks()
	if failing := failingWhere(gated, "dashed"); len(failing) > 0 {
		t.Errorf("a dark dashed outline failed: %s = %.2f:1", failing[0].Selector, failing[0].Ratio)
	}
}

// TestAnalyzeRowMarksOnlyMeasuresTheDefaultHairlines: the default look keeps
// both cursor hairlines subtle by design, so they're reported, not gated.
func TestAnalyzeRowMarksOnlyMeasuresTheDefaultHairlines(t *testing.T) {
	faint := "color-mix(in oklch, var(--color-accent), transparent 50%)"
	gated, measured := NewAnalyzer(rowMarkFixture(faint, "#5a4300")).AnalyzeRowMarks()
	for _, f := range gated {
		if strings.Contains(f.Selector, "hairline") && !strings.Contains(f.Selector, "[") {
			t.Errorf("default-look hairline %q is gated", f.Selector)
		}
	}
	if len(measured) == 0 {
		t.Error("the default-look hairlines aren't measured at all")
	}
}

// TestAnalyzeRowMarksGatesTheMarkedBar: the marked-file bar paints in the
// selection red, so a red too close to the row fails 3:1.
func TestAnalyzeRowMarksGatesTheMarkedBar(t *testing.T) {
	vt := rowMarkFixture("#5a4300", "#5a4300")
	vt.Light["color-selection-fg-primary"] = "#f4dcdc"
	gated, _ := NewAnalyzer(vt).AnalyzeRowMarks()
	if len(failingWhere(gated, "marked-file bar")) == 0 {
		t.Fatal("a pale marked-file bar passed the 3:1 gate")
	}
}
