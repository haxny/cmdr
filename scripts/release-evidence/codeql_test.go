package main

import "testing"

func TestPickAnalysesTakesTheNewestPerLanguageOnTheCommit(t *testing.T) {
	list := `[
	  {"id": 30, "commit_sha": "other", "category": "/language:actions", "created_at": "2026-10-22T10:00:00Z"},
	  {"id": 21, "commit_sha": "abc", "category": "/language:actions", "created_at": "2026-10-21T22:00:00Z"},
	  {"id": 20, "commit_sha": "abc", "category": "/language:actions", "created_at": "2026-10-21T21:00:00Z"},
	  {"id": 25, "commit_sha": "abc", "category": "/language:javascript-typescript", "created_at": "2026-10-21T21:30:00Z"}
	]`
	got, err := pickAnalyses([]byte(list), "abc")
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 2 || got[0].ID != 21 || got[1].ID != 25 {
		t.Errorf("got %+v, want 21 and 25", got)
	}
	if _, err := pickAnalyses([]byte(list), "nope"); err == nil {
		t.Error("no analysis on the commit should fail")
	}
}
