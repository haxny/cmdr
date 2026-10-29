package checks

import (
	"testing"
)

func TestValidateCheckNames_NoCollisions(t *testing.T) {
	// The actual AllChecks should have no collisions
	if err := ValidateCheckNames(); err != nil {
		t.Errorf("ValidateCheckNames() failed on actual registry: %v", err)
	}
}

// TestEveryCheckDeclaresInputs guards the input-fingerprint cache contract: a
// check with no Inputs is fingerprinted on the global inputs alone, so it would
// be cache-skipped on any change to its own files — a correctness hole. Every
// check must declare what it reads (use wholeRepoInputs for whole-tree scanners).
func TestEveryCheckDeclaresInputs(t *testing.T) {
	for _, c := range AllChecks {
		if len(c.Inputs) == 0 {
			t.Errorf("check %q declares no Inputs; the cache can't tell what it reads (see checks/inputs.go)", c.ID)
		}
	}
}

func TestValidateCheckNames_DetectsNicknameIDCollision(t *testing.T) {
	// Save original and restore after test
	original := AllChecks
	defer func() { AllChecks = original }()

	// Create a test case where a nickname conflicts with another check's ID
	AllChecks = []CheckDefinition{
		{ID: "check-a", Nickname: "short-a", DisplayName: "A", App: AppDesktop, Tech: "Test"},
		{ID: "short-a", DisplayName: "B", App: AppDesktop, Tech: "Test"}, // ID collides with check-a's nickname
	}

	err := ValidateCheckNames()
	if err == nil {
		t.Error("ValidateCheckNames() should detect nickname-ID collision")
	}
}

func TestValidateCheckNames_DetectsDuplicateNicknames(t *testing.T) {
	original := AllChecks
	defer func() { AllChecks = original }()

	AllChecks = []CheckDefinition{
		{ID: "check-a", Nickname: "short", DisplayName: "A", App: AppDesktop, Tech: "Test"},
		{ID: "check-b", Nickname: "short", DisplayName: "B", App: AppDesktop, Tech: "Test"}, // Same nickname
	}

	err := ValidateCheckNames()
	if err == nil {
		t.Error("ValidateCheckNames() should detect duplicate nicknames")
	}
}

func TestValidateCheckNames_RejectsReservedSelectorNames(t *testing.T) {
	original := AllChecks
	defer func() { AllChecks = original }()

	AllChecks = []CheckDefinition{
		{ID: "rust", DisplayName: "A", App: AppDesktop, Tech: "Test"},
	}
	if err := ValidateCheckNames("rust", "svelte"); err == nil {
		t.Error("ValidateCheckNames() should reject a check ID that shadows a reserved selector keyword")
	}

	AllChecks = []CheckDefinition{
		{ID: "check-a", Nickname: "svelte", DisplayName: "A", App: AppDesktop, Tech: "Test"},
	}
	if err := ValidateCheckNames("rust", "svelte"); err == nil {
		t.Error("ValidateCheckNames() should reject a nickname that shadows a reserved selector keyword")
	}
}

func TestCLIName(t *testing.T) {
	tests := []struct {
		name     string
		def      CheckDefinition
		expected string
	}{
		{
			name:     "returns nickname when set",
			def:      CheckDefinition{ID: "full-id", Nickname: "short"},
			expected: "short",
		},
		{
			name:     "returns ID when nickname is empty",
			def:      CheckDefinition{ID: "full-id", Nickname: ""},
			expected: "full-id",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.def.CLIName(); got != tt.expected {
				t.Errorf("CLIName() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestGetCheckByID_MatchesNickname(t *testing.T) {
	original := AllChecks
	defer func() { AllChecks = original }()

	AllChecks = []CheckDefinition{
		{ID: "desktop-svelte-check", Nickname: "svelte-check", DisplayName: "svelte-check", App: AppDesktop, Tech: "Test"},
	}

	// Should find by ID
	if check := GetCheckByID("desktop-svelte-check"); check == nil {
		t.Error("GetCheckByID() should find check by ID")
	}

	// Should find by nickname
	if check := GetCheckByID("svelte-check"); check == nil {
		t.Error("GetCheckByID() should find check by nickname")
	}

	// Should not find unknown
	if check := GetCheckByID("unknown"); check != nil {
		t.Error("GetCheckByID() should return nil for unknown check")
	}
}

// TestFilterDisabledChecks_ExcludedUnlessNamed pins the mothball contract: a
// check carrying a Disabled reason leaves every suite, and the ONLY way back is
// naming it. There's deliberately no bulk flag, so a disabled check can't
// rejoin a lane by accident.
func TestFilterDisabledChecks_ExcludedUnlessNamed(t *testing.T) {
	defs := []CheckDefinition{
		{ID: "live-one"},
		{ID: "mothballed", Disabled: "noisy and low value"},
	}

	kept := FilterDisabledChecks(defs, nil)
	if len(kept) != 1 || kept[0].ID != "live-one" {
		t.Errorf("unnamed run kept %v; want just [live-one]", idsOf(kept))
	}
}

// The named-check escape hatch resolves through GetCheckByID, so it has to work
// off the real registry rather than a synthetic slice.
func TestFilterDisabledChecks_NamedCheckRunsAnyway(t *testing.T) {
	var disabled *CheckDefinition
	for i := range AllChecks {
		if AllChecks[i].Disabled != "" {
			disabled = &AllChecks[i]
			break
		}
	}
	if disabled == nil {
		t.Skip("no check is currently mothballed; nothing to assert")
	}

	if kept := FilterDisabledChecks(AllChecks, nil); containsID(kept, disabled.ID) {
		t.Errorf("check %q is Disabled but survived an unnamed run", disabled.ID)
	}
	if kept := FilterDisabledChecks(AllChecks, []string{disabled.ID}); !containsID(kept, disabled.ID) {
		t.Errorf("check %q was named explicitly but was still filtered out", disabled.ID)
	}
}

// A mothballed check must not also claim a lane it can no longer be part of: a
// leftover IsFast/IsSlow/CIOnly reads as "runs in that lane" to anyone skimming
// the registry, and would come back the moment Disabled is lifted.
func TestDisabledChecksClaimNoLane(t *testing.T) {
	for _, c := range AllChecks {
		if c.Disabled == "" {
			continue
		}
		if c.IsFast || c.IsSlow || c.CIOnly {
			t.Errorf("check %q is Disabled but still marked IsFast/IsSlow/CIOnly; clear the lane flags", c.ID)
		}
		if c.NotInCI == "" {
			t.Errorf("check %q is Disabled but has no NotInCI reason; a mothballed check is never in a workflow", c.ID)
		}
	}
}

// Every Astro command that syncs content rewrites `apps/website/.astro/` and
// `node_modules/.astro/data-store.json` through a fixed `<name>.tmp` + rename, so
// two of them at once rename each other's temp file away and one dies on ENOENT.
// The sync → eslint → typecheck → build chain is serial by dependency, but the
// analytics build has none, so only a shared resource keeps it off the others.
func TestAstroWritingLanesNeverOverlap(t *testing.T) {
	for _, id := range []string{"website-astro-sync", "website-typecheck", "website-build", "website-analytics-injection"} {
		c := GetCheckByID(id)
		if c == nil {
			t.Fatalf("check %q not found", id)
		}
		if c.Exclusive != ResourceAstroGeneratedDir {
			t.Errorf("check %q runs an Astro sync but holds Exclusive=%q, want %q", id, c.Exclusive, ResourceAstroGeneratedDir)
		}
	}
}

func idsOf(defs []CheckDefinition) []string {
	ids := make([]string, 0, len(defs))
	for _, d := range defs {
		ids = append(ids, d.ID)
	}
	return ids
}

func containsID(defs []CheckDefinition, id string) bool {
	for _, d := range defs {
		if d.ID == id {
			return true
		}
	}
	return false
}

// The pre-release lane is its own: `--pre-release` runs exactly its checks, every other run leaves
// them out, and naming one still runs it.
func TestFilterPreReleaseChecks(t *testing.T) {
	defs := []CheckDefinition{{ID: "plain"}, {ID: "gate", IsPreRelease: true}}
	if got := FilterPreReleaseChecks(defs, false, nil); containsID(got, "gate") || !containsID(got, "plain") {
		t.Errorf("a normal run should drop the pre-release check and keep the rest, got %v", got)
	}
	if got := FilterPreReleaseChecks(defs, true, nil); !containsID(got, "gate") || containsID(got, "plain") {
		t.Errorf("--pre-release should run only the pre-release check, got %v", got)
	}
}

func TestFilterPreReleaseChecksNamedBypass(t *testing.T) {
	var gate string
	for _, c := range AllChecks {
		if c.IsPreRelease {
			gate = c.ID
			break
		}
	}
	if gate == "" {
		t.Fatal("no check is IsPreRelease; drop the lane or this test")
	}
	if got := FilterPreReleaseChecks(AllChecks, false, []string{gate}); !containsID(got, gate) {
		t.Errorf("naming %q should still run it", gate)
	}
}

// A pre-release check claims no other lane: IsSlow would also pull it into `--include-slow`, and
// IsFast or CIOnly would contradict the "only before a release" promise.
func TestPreReleaseChecksClaimNoOtherLane(t *testing.T) {
	for _, c := range AllChecks {
		if c.IsPreRelease && (c.IsFast || c.IsSlow || c.CIOnly || c.JoinsScopedRuns || c.NamedOnly != "" || c.Disabled != "") {
			t.Errorf("check %q is IsPreRelease but also claims another lane; clear the other lane flags", c.ID)
		}
	}
}

func TestNamedOnlyChecksClaimNoLane(t *testing.T) {
	count := 0
	for _, c := range AllChecks {
		if c.NamedOnly == "" {
			continue
		}
		count++
		if c.IsFast || c.IsSlow || c.CIOnly || c.JoinsScopedRuns {
			t.Errorf("check %q is NamedOnly but still claims a lane; clear the lane flags", c.ID)
		}
		if c.NotInCI == "" {
			t.Errorf("check %q is NamedOnly but has no NotInCI reason; a named-only check is never in a workflow", c.ID)
		}
		if kept := FilterDisabledChecks(AllChecks, nil); containsID(kept, c.ID) {
			t.Errorf("check %q is NamedOnly but an unnamed run kept it", c.ID)
		}
		if kept := FilterDisabledChecks(AllChecks, []string{c.ID}); !containsID(kept, c.ID) {
			t.Errorf("check %q was named explicitly but was still filtered out", c.ID)
		}
	}
	if count == 0 {
		t.Error("no check is NamedOnly; drop the field or this test")
	}
}

// `AGENTS.md` tells agents what `--fast` covers by kind: light scanners, never a
// compiler, typechecker, test suite, or formatter (those all weigh 2+ cores), so
// an agent knows the rest is CI's. This keeps that one line true without listing
// the lane's members anywhere.
func TestFastLaneHoldsOnlyLightScanners(t *testing.T) {
	for _, c := range AllChecks {
		if !c.IsFast {
			continue
		}
		if c.CpuWeight > 1 || c.Tech == "📐 Format" {
			t.Errorf("%s is in --fast but weighs %d cores (tech %s); --fast holds light scanners only, so update AGENTS.md § Checker script if that changes", c.ID, c.CpuWeight, c.Tech)
		}
	}
}

// A lane that joins every scoped run must stay what makes that free: an enabled,
// whole-repo scanner in the fast lane, never a formatter (a local run rewrites
// files, and the git hooks own formatting).
func TestScopedRunLanesAreCheapWholeRepoScanners(t *testing.T) {
	count := 0
	for _, c := range AllChecks {
		if !c.JoinsScopedRuns {
			continue
		}
		count++
		if c.App != AppOther || !c.IsFast || c.Tech == "📐 Format" || c.Disabled != "" {
			t.Errorf("%s joins scoped runs, but it isn't an enabled, fast, non-formatter AppOther lane", c.ID)
		}
	}
	if count == 0 {
		t.Error("no lane joins scoped runs")
	}
}
