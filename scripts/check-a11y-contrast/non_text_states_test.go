package main

import "testing"

func nonTextFixture(fieldBorder string) *VarTable {
	vt := NewVarTable()
	vt.Light["color-bg-primary"] = "#ffffff"
	vt.Light["color-bg-secondary"] = "#f5f5f5"
	vt.Light["color-bg-dialog"] = "#ebebeb"
	vt.Light["color-field-border"] = fieldBorder
	vt.Dark["color-bg-primary"] = "#1e1e1e"
	vt.Dark["color-bg-secondary"] = "#2a2a2a"
	vt.Dark["color-bg-dialog"] = "#232323"
	vt.Dark["color-field-border"] = "#808080"
	vt.Light["color-accent"] = "#d4a006"
	vt.Light["color-accent-pop"] = "color-mix(in oklch, var(--color-accent), black 40%)"
	vt.Dark["color-accent"] = "#ffc206"
	vt.Dark["color-accent-pop"] = "color-mix(in oklch, var(--color-accent), white 40%)"
	vt.ContrastLight["color-focus-ink"] = "#1a1a1a"
	vt.ContrastDark["color-focus-ink"] = "#e8e8e8"
	return vt
}

func failingNonText(findings []Finding) []Finding {
	var out []Finding
	for _, f := range findings {
		if !f.IsPassing {
			out = append(out, f)
		}
	}
	return out
}

// TestAnalyzeNonTextStatesFlagsAFaintFieldBorder is the regression anchor for
// the audit's 1.4.11 finding: text fields drew their edge in the decorative
// `--color-border` (#ddd, 1.36:1 on white), and no check measured a border at
// all. The non-text scenarios hold every field edge to 3:1 on the surfaces
// fields sit on.
func TestAnalyzeNonTextStatesFlagsAFaintFieldBorder(t *testing.T) {
	failing := failingNonText(NewAnalyzer(nonTextFixture("#dddddd")).AnalyzeNonTextStates())
	if len(failing) == 0 {
		t.Fatal("a #ddd field border passed the 3:1 non-text gate")
	}
	for _, f := range failing {
		if f.Threshold != 3.0 {
			t.Errorf("non-text finding %q uses threshold %.1f, want 3.0", f.Selector, f.Threshold)
		}
	}
}

// TestAnalyzeNonTextStatesPassesTheControlBorder is the green twin: the 3:1
// control border clears every surface in both modes.
func TestAnalyzeNonTextStatesPassesTheControlBorder(t *testing.T) {
	if failing := failingNonText(NewAnalyzer(nonTextFixture("#858585")).AnalyzeNonTextStates()); len(failing) > 0 {
		for _, f := range failing {
			t.Errorf("%s (%s) = %.2f:1", f.Selector, f.Mode, f.Ratio)
		}
	}
}
