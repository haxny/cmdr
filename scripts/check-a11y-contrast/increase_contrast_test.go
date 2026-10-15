package main

import "testing"

const sampleIncreaseContrastCSS = `
:root {
  --color-border: #ddd;
  --pane-tint-fg-pct: 10%;
}

@media (prefers-color-scheme: dark) {
  :root {
    --color-border: #444444;
    --pane-tint-fg-pct: 15%;
  }
}

html.increase-contrast {
  --color-border: #949494;
  --pane-tint-fg-pct: 15%;
}

@media (prefers-color-scheme: dark) {
  html.increase-contrast {
    --pane-tint-fg-pct: 25%;
  }
}
`

// TestParseAppCSSKeepsIncreaseContrastOutOfTheBaseTables guards the baseline:
// the `html.increase-contrast` overrides only apply under the class, so they
// must never leak into the default light/dark tables every other check reads.
func TestParseAppCSSKeepsIncreaseContrastOutOfTheBaseTables(t *testing.T) {
	vt := ParseAppCSS(sampleIncreaseContrastCSS)
	if got := vt.Light["color-border"]; got != "#ddd" {
		t.Errorf("light border = %q, want the baseline #ddd", got)
	}
	if got, _ := vt.Raw("pane-tint-fg-pct", ModeDark); got != "15%" {
		t.Errorf("dark tint pct = %q, want the baseline 15%%", got)
	}
}

// TestWithIncreaseContrastLayersTheClassOverrides mirrors the cascade: a
// top-level `html.increase-contrast` rule (specificity 0,1,1) beats the dark
// `:root` (0,1,0) in BOTH modes, and a dark-nested one wins in dark.
func TestWithIncreaseContrastLayersTheClassOverrides(t *testing.T) {
	vt := ParseAppCSS(sampleIncreaseContrastCSS).WithIncreaseContrast()
	cases := []struct {
		name string
		mode Mode
		want string
	}{
		{"color-border", ModeLight, "#949494"},
		{"color-border", ModeDark, "#949494"},
		{"pane-tint-fg-pct", ModeLight, "15%"},
		{"pane-tint-fg-pct", ModeDark, "25%"},
	}
	for _, c := range cases {
		if got, _ := vt.Raw(c.name, c.mode); got != c.want {
			t.Errorf("%s (%s) = %q, want %q", c.name, c.mode, got, c.want)
		}
	}
}

// TestResolvePaneBgReadsTheTintShareFromTheToken pins the pane tint share to
// `--pane-tint-fg-pct` instead of a hardcoded 10/15: the Increase contrast
// tiers (15% light, 25% dark) only reach the matrix through that token.
func TestResolvePaneBgReadsTheTintShareFromTheToken(t *testing.T) {
	brown := paneTintHue{Name: "brown", VarName: "color-tint-brown"}
	white := RGBA{R: 255, G: 255, B: 255, A: 1}
	base := NewVarTable()
	base.Light["color-bg-primary"] = "#ffffff"
	base.Light["color-tint-brown"] = "#92400e"
	base.Light["pane-tint-fg-pct"] = "10%"
	base.ContrastLight["pane-tint-fg-pct"] = "15%"

	normal, ok := resolvePaneBg(base, ModeLight, brown)
	if !ok {
		t.Fatal("normal pane bg didn't resolve")
	}
	raised, ok := resolvePaneBg(base.WithIncreaseContrast(), ModeLight, brown)
	if !ok {
		t.Fatal("increase-contrast pane bg didn't resolve")
	}
	if ContrastRatio(raised, white) <= ContrastRatio(normal, white) {
		t.Errorf("15%% tint %s should sit further from white than 10%% tint %s", raised.Hex(), normal.Hex())
	}
}

// TestAnalyzeRowStatesSweepsIncreaseContrast is the regression anchor for the
// gap the audit found: with macOS Increase contrast on, marked names on a
// raised pane tint were never evaluated, so a 4.18:1 brown corner shipped
// unseen. The matrix now evaluates every row cell a second time under the
// class and tags those findings.
func TestAnalyzeRowStatesSweepsIncreaseContrast(t *testing.T) {
	vt := NewVarTable()
	vt.Light["color-bg-primary"] = "#ffffff"
	vt.Light["color-tint-brown"] = "#92400e"
	vt.Light["pane-tint-fg-pct"] = "10%"
	vt.Light["color-selection-fg-primary"] = "#cc0000"
	vt.Light["color-selection-fg"] = "var(--color-selection-fg-primary)"
	vt.ContrastLight["pane-tint-fg-pct"] = "15%"

	findings := NewAnalyzer(vt).AnalyzeRowStates()
	want := describeRowScenario("color-selection-fg", paneTintHue{Name: "brown"}, "plain", true) + " [increase-contrast]"
	for _, f := range findings {
		if f.Selector == want && f.Mode == ModeLight {
			return
		}
	}
	t.Errorf("no increase-contrast finding %q", want)
}
