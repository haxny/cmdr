package main

import (
	"fmt"
	"slices"
	"strings"
	"time"
)

// A workflow run as `gh run list --json databaseId,event,status,conclusion,createdAt,headSha` gives it.
type workflowRun struct {
	ID         int64     `json:"databaseId"`
	Event      string    `json:"event"`
	Status     string    `json:"status"`
	Conclusion string    `json:"conclusion"`
	CreatedAt  time.Time `json:"createdAt"`
	HeadSHA    string    `json:"headSha"`
}

func (r workflowRun) succeeded() bool { return r.Status == "completed" && r.Conclusion == "success" }

// A job as `gh run view --json jobs` gives it.
type runJob struct {
	Name       string `json:"name"`
	Status     string `json:"status"`
	Conclusion string `json:"conclusion"`
}

// fullRunMarkerJob is the `ci.yml` job that runs only with `run_all`: the runs API carries no
// inputs, so a dispatch counts as a full run only when this job didn't skip. Same rule as
// `scripts/release-ci-gate.sh`.
const fullRunMarkerJob = "Full run (run_all)"

// attestJobSuffix names release-pipeline.yml's `attest` job. Inside the reusable workflow, job
// names show as `Release / <name>`, hence a suffix.
const attestJobSuffix = "Attest provenance and publish SBOMs"

// ciCandidate is one commit whose full CI run the release gate may have passed: the tagged
// `chore(release)` commit first, then its parent.
type ciCandidate struct {
	commit string
	// how the sub-bullet names the commit, like "the parent of the `chore(release)` commit"
	describe string
	runs     []workflowRun
	// jobs per run id, for the marker job
	jobs map[int64][]runJob
}

func jobConclusion(jobs []runJob, name string) string {
	for _, j := range jobs {
		if j.Name == name {
			return j.Conclusion
		}
	}
	return ""
}

func newestFirst(runs []workflowRun) []workflowRun {
	sorted := slices.Clone(runs)
	slices.SortFunc(sorted, func(a, b workflowRun) int { return b.CreatedAt.Compare(a.CreatedAt) })
	return sorted
}

// newest is the most recently created of runs, which mustn't be empty.
func newest(runs []workflowRun) workflowRun {
	var n workflowRun
	for i, r := range runs {
		if i == 0 || r.CreatedAt.After(n.CreatedAt) {
			n = r
		}
	}
	return n
}

// pickFullRun finds the green full `ci.yml` run the release rests on: the newest on the first
// candidate that has one. Today the gate passes on the release commit's parent; once the release
// commit gets its own full run, that one wins.
func pickFullRun(cands []ciCandidate) (ciCandidate, workflowRun, error) {
	var seen []string
	for _, c := range cands {
		for _, r := range newestFirst(c.runs) {
			if r.Event != "workflow_dispatch" || jobConclusion(c.jobs[r.ID], fullRunMarkerJob) != "success" {
				continue
			}
			if r.succeeded() {
				return c, r, nil
			}
			seen = append(seen, fmt.Sprintf("run %d on %s ended %q", r.ID, short(c.commit), r.Status+"/"+r.Conclusion))
		}
	}
	commits := make([]string, len(cands))
	for i, c := range cands {
		commits[i] = short(c.commit)
	}
	msg := "no green full (`run_all`) ci.yml run on " + strings.Join(commits, " or ")
	if len(seen) > 0 {
		msg += "; full runs seen: " + strings.Join(seen, ", ")
	}
	return ciCandidate{}, workflowRun{}, fmt.Errorf("%s. The release gate should have left one; read `gh run list --workflow ci.yml --commit <sha>`", msg)
}

// pickCodeQLRun is the newest `codeql.yml` dispatch on the commit (the release gate's run), or the
// newest run of any event when there's no dispatch. It must be green.
func pickCodeQLRun(runs []workflowRun, commit string) (workflowRun, error) {
	if len(runs) == 0 {
		return workflowRun{}, fmt.Errorf("no codeql.yml run on %s, the commit the full CI run passed on", short(commit))
	}
	// The gate's run is the dispatch `release.sh` (or a security patch's step 4) started. A later PR
	// run on the same commit, like the merge-back's, mustn't displace it. Releases from before the
	// dispatch had push runs only, so they fall back to every run.
	candidates := runs
	var dispatches []workflowRun
	for _, r := range runs {
		if r.Event == "workflow_dispatch" {
			dispatches = append(dispatches, r)
		}
	}
	if len(dispatches) > 0 {
		candidates = dispatches
	}
	r := newest(candidates)
	if !r.succeeded() {
		return workflowRun{}, fmt.Errorf("the newest codeql.yml run the release gate could have checked on %s (%d) is %s/%s, not green", short(commit), r.ID, r.Status, r.Conclusion)
	}
	return r, nil
}

// pickReleaseRuns is the release's `release.yml` runs: the newest tag push run and, in `local`
// mode, the newest dispatch that finished it. Each must be green and on the tagged commit.
func pickReleaseRuns(runs []workflowRun, commit string) ([]workflowRun, error) {
	var picked []workflowRun
	for _, event := range []string{"push", "workflow_dispatch"} {
		var ofEvent []workflowRun
		for _, r := range runs {
			if r.Event == event {
				ofEvent = append(ofEvent, r)
			}
		}
		if len(ofEvent) == 0 {
			continue
		}
		r := newest(ofEvent)
		if !r.succeeded() {
			return nil, fmt.Errorf("the newest release.yml %s run (%d) is %s/%s, not green", eventLabel(event), r.ID, r.Status, r.Conclusion)
		}
		if r.HeadSHA != commit {
			return nil, fmt.Errorf("release.yml run %d built %s, not the tagged commit %s", r.ID, short(r.HeadSHA), short(commit))
		}
		picked = append(picked, r)
	}
	if len(picked) == 0 || picked[0].Event != "push" {
		return nil, fmt.Errorf("no release.yml tag push run for the tag")
	}
	return picked, nil
}

// checkAttestJob requires the run's `attest` job green. A red one doesn't unpublish anything,
// but the evidence rests on it: `docs/guides/releasing.md` § The attest or sbom job failed.
func checkAttestJob(runID int64, jobs []runJob) error {
	for _, j := range jobs {
		if strings.HasSuffix(j.Name, attestJobSuffix) {
			if j.Conclusion != "success" {
				return fmt.Errorf("release.yml run %d's attest job ended %q. Read docs/guides/releasing.md § The attest or sbom job failed", runID, j.Conclusion)
			}
			return nil
		}
	}
	return fmt.Errorf("release.yml run %d has no %q job", runID, attestJobSuffix)
}

func eventLabel(event string) string {
	if event == "push" {
		return "tag push"
	}
	return "dispatch"
}

func short(sha string) string {
	if len(sha) > 9 {
		return sha[:9]
	}
	return sha
}
