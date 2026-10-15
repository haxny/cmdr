package main

import "fmt"

// Row marks: the non-text cues (WCAG 1.4.11) painted on file-list rows,
// swept like the row-state matrix (mode × pane tint × row bg variant × accent)
// in each look a scenario names.
//
// These are the cues that tell states apart without hue (GitHub issue #440):
//   - The marked-file bar (`app-file-list.css`), painted in the same
//     `--color-selection-fg` cascade as the marked text, so it shifts with the
//     cursor exactly as the text does.
//   - The cursor hairlines: solid in the focused pane, dashed in the other.
//     Solid vs dashed is the focused-pane cue. By default the solid one is the
//     faint accent and the dashed one a faint gray (David's picks, 2026-10-15),
//     both measured, not gated. Increase contrast lifts the solid one to
//     `--color-accent-pop` and the dashed one to the full control-border gray;
//     Differentiate without color lifts both to `--color-accent-pop`. There they
//     must clear 3:1.
//
// A gated finding joins the WCAG gate; a measured one prints under
// `--verbose` only.

type rowMarkScenario struct {
	// Name labels the mark in the report.
	Name string
	// MarkVar returns the token the mark paints in for one row cell.
	MarkVar func(cell rowCell) string
	// Variants are the row bg variants the mark appears on.
	Variants []string
	// Selected marks a scenario whose row is marked, so a plain or striped
	// row shows `--color-selection-bg`.
	Selected bool
	// Gated looks are held to 3:1; Measured looks are only reported.
	Gated, Measured []rowContrastTier
}

func constMark(name string) func(rowCell) string {
	return func(rowCell) string { return name }
}

var rowMarkScenarios = []rowMarkScenario{
	{
		Name: "marked-file bar",
		MarkVar: func(c rowCell) string {
			return selectionFgTokenFor(c.mode, c.tint, c.variant, c.tier.increase)
		},
		Variants: rowBgVariants,
		Selected: true,
		Gated:    []rowContrastTier{tierDefault, tierIncrease, tierDifferentiate},
	},
	{
		Name:     "focused cursor hairline (solid)",
		MarkVar:  constMark("color-cursor-outline"),
		Variants: []string{"cursor-active"},
		Gated:    []rowContrastTier{tierIncrease, tierDifferentiate},
		Measured: []rowContrastTier{tierDefault},
	},
	{
		Name:     "unfocused cursor hairline (dashed)",
		MarkVar:  constMark("color-cursor-outline-dashed"),
		Variants: []string{"cursor-inactive"},
		Gated:    []rowContrastTier{tierIncrease, tierDifferentiate},
		Measured: []rowContrastTier{tierDefault},
	},
}

// AnalyzeRowMarks returns the worst finding per (scenario, look, mode), split
// into the gated ones and the measured-only ones.
func (a *Analyzer) AnalyzeRowMarks() (gated, measured []Finding) {
	for _, sc := range rowMarkScenarios {
		for _, tier := range sc.Gated {
			gated = append(gated, a.worstRowMarkPerMode(sc, tier)...)
		}
		for _, tier := range sc.Measured {
			measured = append(measured, a.worstRowMarkPerMode(sc, tier)...)
		}
	}
	return gated, measured
}

func (a *Analyzer) worstRowMarkPerMode(sc rowMarkScenario, tier rowContrastTier) []Finding {
	base := tableFor(a.Vars, tier)
	var out []Finding
	for _, mode := range []Mode{ModeLight, ModeDark} {
		worst := Finding{File: syntheticRowMarkPath(), Mode: mode, Threshold: 3.0, Ratio: -1}
		for _, tint := range rowPaneTints {
			for _, variant := range sc.Variants {
				cell := rowCell{mode: mode, tint: tint, variant: variant, tier: tier}
				if !a.foldRowMark(base, sc, cell, &worst) {
					return append(out, worst)
				}
			}
		}
		worst.IsPassing = worst.Ratio >= worst.Threshold
		out = append(out, worst)
	}
	return out
}

// foldRowMark sweeps the accent variants for one cell and lowers `worst` to
// the cell's lowest ratio. Returns false (with `worst` marked failing) when a
// token doesn't resolve: a renamed token must not turn the gate green.
func (a *Analyzer) foldRowMark(base *VarTable, sc rowMarkScenario, cell rowCell, worst *Finding) bool {
	markVar := sc.MarkVar(cell)
	for _, accent := range AccentVariants {
		vars := base
		if !accent.IsDefault {
			vars = withAccentOverride(base, accent)
		}
		a.RulesEvaluated++
		bg, okBg := resolveRowBg(vars, cell.mode, cell.tint, cell.variant, sc.Selected)
		mark, okMark := resolveVar(vars, cell.mode, markVar)
		if !okBg || !okMark {
			worst.Selector = describeRowMark(sc.Name, cell)
			worst.Ratio, worst.IsPassing = 0, false
			worst.Warnings = append(worst.Warnings, fmt.Sprintf("%s: couldn't resolve --%s or the row bg", worst.Selector, markVar))
			return false
		}
		if !mark.Opaque() {
			mark = CompositeOver(mark, bg)
		}
		ratio := ContrastRatio(mark, bg)
		if worst.Ratio < 0 || ratio < worst.Ratio {
			worst.FG, worst.BG, worst.Ratio = mark, bg, ratio
			worst.Selector = describeRowMark(sc.Name, cell)
			worst.AccentVariant = ""
			if !accent.IsDefault {
				worst.AccentVariant = accent.Name
			}
		}
	}
	return true
}

func describeRowMark(name string, cell rowCell) string {
	return fmt.Sprintf("%s (worst: tint=%s, %s)%s", name, cell.tint.Name, cell.variant, tierTag(cell.tier))
}

func syntheticRowMarkPath() string {
	return "scripts/check-a11y-contrast/row_mark_states.go"
}
