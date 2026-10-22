package main

import (
	"testing"
	"time"
)

var t0 = time.Date(2026, 10, 21, 22, 0, 0, 0, time.UTC)

func dispatch(id int64, conclusion string, minutes int) workflowRun {
	return workflowRun{ID: id, Event: "workflow_dispatch", Status: "completed", Conclusion: conclusion, CreatedAt: t0.Add(time.Duration(minutes) * time.Minute)}
}

func fullMarker(conclusion string) []runJob {
	return []runJob{{Name: "Detect changes", Conclusion: "success"}, {Name: fullRunMarkerJob, Conclusion: conclusion}}
}

func TestPickFullRunTakesTheParentsRunWhenTheReleaseCommitHasNone(t *testing.T) {
	cands := []ciCandidate{
		{commit: "tagged", describe: "the tagged commit"},
		{commit: "parent", describe: "the parent",
			runs: []workflowRun{dispatch(1, "success", 0), {ID: 2, Event: "push", Status: "completed", Conclusion: "success", CreatedAt: t0}},
			jobs: map[int64][]runJob{1: fullMarker("success")}},
	}
	c, r, err := pickFullRun(cands)
	if err != nil || c.commit != "parent" || r.ID != 1 {
		t.Fatalf("got %s %d %v, want parent run 1", c.commit, r.ID, err)
	}
}

func TestPickFullRunPrefersAFullRunOnTheReleaseCommit(t *testing.T) {
	cands := []ciCandidate{
		{commit: "tagged", runs: []workflowRun{dispatch(5, "success", 30)}, jobs: map[int64][]runJob{5: fullMarker("success")}},
		{commit: "parent", runs: []workflowRun{dispatch(1, "success", 0)}, jobs: map[int64][]runJob{1: fullMarker("success")}},
	}
	c, r, err := pickFullRun(cands)
	if err != nil || c.commit != "tagged" || r.ID != 5 {
		t.Fatalf("got %s %d %v, want tagged run 5", c.commit, r.ID, err)
	}
}

func TestPickFullRunSkipsDispatchesWithoutRunAllAndRedRuns(t *testing.T) {
	cands := []ciCandidate{
		// A red full run on the release commit, and a newer dispatch with run_all off.
		{commit: "tagged", runs: []workflowRun{dispatch(5, "failure", 30), dispatch(6, "success", 40)},
			jobs: map[int64][]runJob{5: fullMarker("success"), 6: fullMarker("skipped")}},
		// An older green full run and a newer red one on the parent: the green one counts.
		{commit: "parent", runs: []workflowRun{dispatch(1, "success", 0), dispatch(2, "cancelled", 10)},
			jobs: map[int64][]runJob{1: fullMarker("success"), 2: fullMarker("success")}},
	}
	c, r, err := pickFullRun(cands)
	if err != nil || c.commit != "parent" || r.ID != 1 {
		t.Fatalf("got %s %d %v, want parent run 1", c.commit, r.ID, err)
	}
}

func TestPickFullRunFailsWithNoGreenFullRun(t *testing.T) {
	cands := []ciCandidate{
		{commit: "tagged"},
		{commit: "parent", runs: []workflowRun{dispatch(1, "failure", 0)}, jobs: map[int64][]runJob{1: fullMarker("success")}},
	}
	if _, _, err := pickFullRun(cands); err == nil {
		t.Fatal("want an error")
	}
}

func TestPickCodeQLRunNeedsTheNewestGreen(t *testing.T) {
	runs := []workflowRun{
		{ID: 1, Status: "completed", Conclusion: "success", CreatedAt: t0},
		{ID: 2, Status: "completed", Conclusion: "failure", CreatedAt: t0.Add(time.Minute)},
	}
	if _, err := pickCodeQLRun(runs, "abc"); err == nil {
		t.Error("a red newest run should fail")
	}
	if r, err := pickCodeQLRun(runs[:1], "abc"); err != nil || r.ID != 1 {
		t.Errorf("got %d %v", r.ID, err)
	}
	if _, err := pickCodeQLRun(nil, "abc"); err == nil {
		t.Error("no run should fail")
	}
}

func releaseRun(id int64, event, conclusion, sha string, minutes int) workflowRun {
	return workflowRun{ID: id, Event: event, Status: "completed", Conclusion: conclusion, HeadSHA: sha, CreatedAt: t0.Add(time.Duration(minutes) * time.Minute)}
}

func TestPickReleaseRuns(t *testing.T) {
	local := []workflowRun{releaseRun(2, "workflow_dispatch", "success", "c", 60), releaseRun(1, "push", "success", "c", 0)}
	got, err := pickReleaseRuns(local, "c")
	if err != nil || len(got) != 2 || got[0].ID != 1 || got[1].ID != 2 {
		t.Errorf("local mode: got %+v %v, want the push then the dispatch", got, err)
	}

	ci := []workflowRun{releaseRun(1, "push", "success", "c", 0)}
	if got, err := pickReleaseRuns(ci, "c"); err != nil || len(got) != 1 {
		t.Errorf("ci mode: got %+v %v", got, err)
	}

	retried := []workflowRun{releaseRun(1, "push", "success", "c", 0), releaseRun(2, "workflow_dispatch", "failure", "c", 30), releaseRun(3, "workflow_dispatch", "success", "c", 60)}
	if got, err := pickReleaseRuns(retried, "c"); err != nil || got[1].ID != 3 {
		t.Errorf("a newer green dispatch should replace a red one: got %+v %v", got, err)
	}

	for name, runs := range map[string][]workflowRun{
		"red newest dispatch": {releaseRun(1, "push", "success", "c", 0), releaseRun(2, "workflow_dispatch", "failure", "c", 60)},
		"wrong commit":        {releaseRun(1, "push", "success", "other", 0)},
		"no tag push":         {releaseRun(2, "workflow_dispatch", "success", "c", 60)},
		"still running":       {{ID: 1, Event: "push", Status: "in_progress", HeadSHA: "c"}},
	} {
		if _, err := pickReleaseRuns(runs, "c"); err == nil {
			t.Errorf("%s: want an error", name)
		}
	}
}

func TestCheckAttestJob(t *testing.T) {
	green := []runJob{{Name: "Release / Publish release", Conclusion: "skipped"}, {Name: "Release / " + attestJobSuffix, Conclusion: "success"}}
	if err := checkAttestJob(1, green); err != nil {
		t.Error(err)
	}
	if err := checkAttestJob(1, []runJob{{Name: "Release / " + attestJobSuffix, Conclusion: "failure"}}); err == nil {
		t.Error("a red attest job should fail")
	}
	if err := checkAttestJob(1, green[:1]); err == nil {
		t.Error("a missing attest job should fail")
	}
}
