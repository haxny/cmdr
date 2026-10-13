package checks

import (
	"reflect"
	"strings"
	"testing"
)

func TestJudgeScorecard(t *testing.T) {
	baseline := map[string]scorecardFloor{
		"Pinned-Dependencies": {Floor: 9, Accepted: []scorecardAccepted{
			{Warning: "Warn: npmCommand not pinned by hash: apps/website/Dockerfile", Reason: "pnpm bootstrap"},
		}},
		"License": {Floor: 9},
		"SAST":    {Floor: 10},
		"Fuzzing": {Floor: 10},
		"Token-Permissions": {Floor: 10, Accepted: []scorecardAccepted{
			{Warning: "Warn: jobLevel 'contents' permission set to 'write': .github/workflows/gone.yml", Reason: "stale"},
		}},
	}
	results := []scorecardCheckResult{
		{Name: "Pinned-Dependencies", Score: 9, Reason: "dependency not pinned by hash detected", Details: []string{
			"Info:  56 out of  56 GitHubAction dependencies pinned",
			"Warn: npmCommand not pinned by hash: apps/website/Dockerfile:7", // accepted, line number aside
			"Warn: containerImage not pinned by hash: probe/Dockerfile:1",    // new
		}},
		{Name: "License", Score: 10, Reason: "license file detected"},
		{Name: "SAST", Score: -1, Reason: "internal error: open file"},
		{Name: "Token-Permissions", Score: 10},
		// Fuzzing is missing from the results entirely.
	}

	got := judgeScorecard(results, baseline)

	if !reflect.DeepEqual(got.raised, map[string]int{"License": 10}) {
		t.Errorf("raised = %v, want License up to 10", got.raised)
	}
	wantStale := []string{"Token-Permissions: Warn: jobLevel 'contents' permission set to 'write': .github/workflows/gone.yml"}
	if !reflect.DeepEqual(got.stale, wantStale) {
		t.Errorf("stale = %v, want %v", got.stale, wantStale)
	}
	joined := strings.Join(got.regressions, "\n")
	if len(got.regressions) != 3 {
		t.Fatalf("want 3 regressions (a new warning, an error, a missing check), got %d:\n%s", len(got.regressions), joined)
	}
	for _, want := range []string{
		"Pinned-Dependencies: new finding: Warn: containerImage not pinned by hash: probe/Dockerfile:1",
		"SAST couldn't be scored: internal error: open file",
		"Fuzzing wasn't scored",
	} {
		if !strings.Contains(joined, want) {
			t.Errorf("regressions missing %q:\n%s", want, joined)
		}
	}
	if strings.Contains(joined, "apps/website/Dockerfile") {
		t.Errorf("an accepted warning must not be reported, whatever its line number:\n%s", joined)
	}
}

func TestJudgeScorecardAcceptsAWarningWhateverItsRemediation(t *testing.T) {
	got := judgeScorecard(
		[]scorecardCheckResult{{Name: "Pinned-Dependencies", Score: 9, Details: []string{
			"Warn: containerImage not pinned by hash: infra/x/Dockerfile:3: pin your Docker image by updating alpine:3.20 to alpine:3.20@sha256:d9e8",
		}}},
		map[string]scorecardFloor{"Pinned-Dependencies": {Floor: 9, Accepted: []scorecardAccepted{
			{Warning: "Warn: containerImage not pinned by hash: infra/x/Dockerfile", Reason: "test"},
		}}},
	)
	if len(got.regressions) != 0 || len(got.stale) != 0 {
		t.Errorf("the line number and the remediation hint are not part of the finding, got %+v", got)
	}
}

func TestJudgeScorecardScoreDrop(t *testing.T) {
	got := judgeScorecard(
		[]scorecardCheckResult{{Name: "License", Score: 0, Reason: "license file not detected"}},
		map[string]scorecardFloor{"License": {Floor: 9}},
	)
	if len(got.regressions) != 1 || !strings.Contains(got.regressions[0], "License 0 < 9: license file not detected") {
		t.Errorf("a score below its floor must fail, got %v", got.regressions)
	}
}

func TestJudgeScorecardAllAtBaseline(t *testing.T) {
	got := judgeScorecard(
		[]scorecardCheckResult{{Name: "SAST", Score: 10}},
		map[string]scorecardFloor{"SAST": {Floor: 10}},
	)
	if len(got.regressions) != 0 || len(got.raised) != 0 || len(got.stale) != 0 {
		t.Errorf("at baseline: want nothing to report, got %+v", got)
	}
}
