package main

import (
	"slices"
	"testing"
)

// A merge commit on 2026-10-03 brings in a branch commit from 2026-10-02: that day's snapshot is
// `main` as it was then (the 2026-10-01 tip), never the branch commit, which `main` didn't have yet.
func TestGroupCommitsByDateSnapshotsMainsOwnTips(t *testing.T) {
	tips := []commit{
		{hash: "merge", date: "2026-10-03", messages: []string{"Merge pull request #1"}},
		{hash: "late1", date: "2026-10-01", messages: []string{"feat: late"}},
		{hash: "early1", date: "2026-10-01", messages: []string{"feat: early"}},
	}
	all := []commit{
		{hash: "branch", date: "2026-10-02", messages: []string{"feat: branch"}},
		{hash: "late1", date: "2026-10-01", messages: []string{"feat: late"}},
		{hash: "early1", date: "2026-10-01", messages: []string{"feat: early"}},
	}
	daily := groupCommitsByDate(tips, all)

	if got := daily["2026-10-01"]; got.hash != "late1" || !slices.Equal(got.messages, []string{"feat: late", "feat: early"}) {
		t.Errorf("2026-10-01: got %+v, want the day's last tip and both messages", got)
	}
	if got := daily["2026-10-02"]; got.hash != "late1" || !slices.Equal(got.messages, []string{"feat: branch"}) {
		t.Errorf("2026-10-02: got %+v, want the earlier tip with the branch commit's message", got)
	}
	if got := daily["2026-10-03"]; got.hash != "merge" || len(got.messages) != 0 {
		t.Errorf("2026-10-03: got %+v, want the merge as the snapshot and no merge message", got)
	}
}
