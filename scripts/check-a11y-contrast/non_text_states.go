package main

import "fmt"

// Non-text contrast (WCAG 1.4.11): the edges and rings that carry a control's
// shape or state, held to 3:1 against the surfaces they sit on.
//
// Text pairs are the rule walker's job; nothing there measures a border or a
// focus ring, so a field edge drawn in the decorative `--color-border` (1.36:1
// on white) shipped unseen. Each scenario names the mark's token and the
// surfaces it meets, and runs in both modes, under every accent variant, and
// (for `tiers`) under macOS Increase contrast too. These findings stay out of
// the APCA text floor: APCA's Lc scale is for text, not a 1px edge.

type nonTextScenario struct {
	// Selector is the human-readable owner of the mark, for the report.
	Selector string
	// MarkVar is the token the edge or ring is painted in (without `--`).
	MarkVar string
	// Surfaces are the bg tokens the mark sits on.
	Surfaces []string
	// IncreaseContrastOnly evaluates the scenario under `html.increase-contrast`
	// alone: a mark the default look doesn't paint (the two-layer focus ring).
	IncreaseContrastOnly bool
}

// fieldSurfaces are the backgrounds a text field sits on: the main window, a
// card or the settings pane, and a dialog panel.
var fieldSurfaces = []string{"color-bg-primary", "color-bg-secondary", "color-bg-dialog"}

var nonTextScenarios = []nonTextScenario{
	{Selector: ".text-field / Combobox / NumberInput edge", MarkVar: "color-field-border", Surfaces: fieldSurfaces},
	// The Differentiate without color pane bar (on the `--color-bg-secondary` header) and
	// the Increase contrast cursor hairline (`app-visual-cues.css`, `app.css`).
	{Selector: "accent-pop state mark", MarkVar: "color-accent-pop", Surfaces: fieldSurfaces},
	{
		Selector:             "focus ring ink layer (html.increase-contrast)",
		MarkVar:              "color-focus-ink",
		Surfaces:             fieldSurfaces,
		IncreaseContrastOnly: true,
	},
}

// AnalyzeNonTextStates evaluates every non-text scenario and returns the
// worst-case finding per (scenario, surface, mode, contrast tier).
func (a *Analyzer) AnalyzeNonTextStates() []Finding {
	var out []Finding
	for _, sc := range nonTextScenarios {
		for _, increase := range []bool{false, true} {
			if sc.IncreaseContrastOnly && !increase {
				continue
			}
			base := a.Vars
			if increase {
				base = a.Vars.WithIncreaseContrast()
			}
			for _, surface := range sc.Surfaces {
				for _, mode := range []Mode{ModeLight, ModeDark} {
					out = append(out, a.worstNonText(base, sc, surface, mode, rowContrastTier{increase: increase}))
				}
			}
		}
	}
	return out
}

// worstNonText sweeps the accent variants for one (scenario, surface, mode,
// tier) and returns the lowest ratio. A token that doesn't resolve is a
// failure, not a skip: a renamed token must not turn the gate green.
func (a *Analyzer) worstNonText(base *VarTable, sc nonTextScenario, surface string, mode Mode, tier rowContrastTier) Finding {
	selector := fmt.Sprintf("%s on %s%s", sc.Selector, surface, tierTag(tier))
	worst := Finding{File: syntheticNonTextPath(), Selector: selector, Mode: mode, Threshold: 3.0, Ratio: -1}
	for _, accent := range AccentVariants {
		vars := base
		if !accent.IsDefault {
			vars = withAccentOverride(base, accent)
		}
		a.RulesEvaluated++
		bg, okBg := resolveVar(vars, mode, surface)
		mark, okMark := resolveVar(vars, mode, sc.MarkVar)
		if !okBg || !okMark {
			worst.Ratio, worst.IsPassing = 0, false
			worst.Warnings = append(worst.Warnings, fmt.Sprintf("%s: couldn't resolve --%s or --%s", selector, sc.MarkVar, surface))
			return worst
		}
		if !bg.Opaque() {
			if primary, ok := resolveVar(vars, mode, "color-bg-primary"); ok {
				bg = CompositeOver(bg, primary)
			}
		}
		if !mark.Opaque() {
			mark = CompositeOver(mark, bg)
		}
		ratio := ContrastRatio(mark, bg)
		if worst.Ratio < 0 || ratio < worst.Ratio {
			worst.FG, worst.BG, worst.Ratio = mark, bg, ratio
			worst.AccentVariant = ""
			if !accent.IsDefault {
				worst.AccentVariant = accent.Name
			}
		}
	}
	worst.IsPassing = worst.Ratio >= worst.Threshold
	return worst
}

func syntheticNonTextPath() string {
	return "scripts/check-a11y-contrast/non_text_states.go"
}
