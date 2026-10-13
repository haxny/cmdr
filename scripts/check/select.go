package main

import (
	"fmt"
	"strings"

	"cmdr/scripts/check/checks"
)

// selectChecks determines which checks to run based on flags.
// Selectors are additive: `pnpm check clippy svelte` runs clippy plus all Svelte checks.
func selectChecks(flags *cliFlags) ([]checks.CheckDefinition, error) {
	hasFilter := len(flags.checkNames) > 0 || len(flags.appNames) > 0 || flags.rustOnly || flags.svelteOnly || flags.goOnly
	if !hasFilter {
		return checks.AllChecks, nil
	}

	seen := make(map[string]bool)
	var result []checks.CheckDefinition
	addUnique := func(cs []checks.CheckDefinition) {
		for _, c := range cs {
			if !seen[c.ID] {
				seen[c.ID] = true
				result = append(result, c)
			}
		}
	}

	if len(flags.checkNames) > 0 {
		named, err := selectChecksByID(flags.checkNames)
		if err != nil {
			return nil, err
		}
		addUnique(named)
	}
	for _, appName := range flags.appNames {
		byApp, err := selectChecksByApp(appName)
		if err != nil {
			return nil, err
		}
		addUnique(byApp)
	}
	if flags.rustOnly {
		addUnique(checks.GetChecksByTech(checks.AppDesktop, "🦀 Rust"))
	}
	if flags.svelteOnly {
		addUnique(checks.GetChecksByTech(checks.AppDesktop, "🎨 Svelte"))
	}
	if flags.goOnly {
		addUnique(checks.GetChecksByTech(checks.AppScripts, "🐹 Go"))
	}
	addUnique(scopedRunLanes(flags))

	return result, nil
}

// scopedRunLanes is the cheap whole-repo doc and size lanes a scoped local run
// adds, so it can't go green over a dead doc link or an over-budget CLAUDE.md that
// only a bare run would catch. CI gets exactly what each job names (the doc lanes
// have a job of their own), and so does `--exact`.
func scopedRunLanes(flags *cliFlags) []checks.CheckDefinition {
	if flags.ciMode || flags.exact {
		return nil
	}
	return checks.ScopedRunLanes()
}

// selectChecksByID returns checks matching the given IDs.
func selectChecksByID(names []string) ([]checks.CheckDefinition, error) {
	var result []checks.CheckDefinition
	for _, name := range names {
		check := checks.GetCheckByID(name)
		if check == nil {
			return nil, fmt.Errorf("unknown check ID: %s\nRun with --help to see available checks", name)
		}
		result = append(result, *check)
	}
	return result, nil
}

// selectChecksByApp returns checks for the given app name.
func selectChecksByApp(appName string) ([]checks.CheckDefinition, error) {
	switch strings.ToLower(appName) {
	case "desktop":
		return checks.GetChecksByApp(checks.AppDesktop), nil
	case "website":
		return checks.GetChecksByApp(checks.AppWebsite), nil
	case "api-server":
		return checks.GetChecksByApp(checks.AppApiServer), nil
	case "dashboard":
		return checks.GetChecksByApp(checks.AppDashboard), nil
	case "scripts":
		return checks.GetChecksByApp(checks.AppScripts), nil
	case "crates":
		return checks.GetChecksByApp(checks.AppCrates), nil
	default:
		return nil, fmt.Errorf("unknown app: %s\nAvailable apps: desktop, website, api-server, dashboard, scripts, crates", appName)
	}
}
