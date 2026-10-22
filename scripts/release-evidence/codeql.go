package main

import (
	"cmp"
	"encoding/json"
	"fmt"
	"slices"
	"time"
)

type codeQLAnalysis struct {
	ID        int64     `json:"id"`
	CommitSHA string    `json:"commit_sha"`
	Category  string    `json:"category"`
	CreatedAt time.Time `json:"created_at"`
}

// pickAnalyses picks the commit's CodeQL analyses from the code-scanning API's list, the newest
// per language category (a re-run uploads a second one). The API has no commit filter.
func pickAnalyses(body []byte, commit string) ([]codeQLAnalysis, error) {
	var all []codeQLAnalysis
	if err := json.Unmarshal(body, &all); err != nil {
		return nil, fmt.Errorf("the code-scanning analyses list isn't JSON: %w", err)
	}
	newest := map[string]codeQLAnalysis{}
	for _, a := range all {
		if a.CommitSHA != commit {
			continue
		}
		if have, ok := newest[a.Category]; !ok || a.CreatedAt.After(have.CreatedAt) {
			newest[a.Category] = a
		}
	}
	if len(newest) == 0 {
		return nil, fmt.Errorf("no CodeQL analysis on %s among the newest 100", short(commit))
	}
	picked := make([]codeQLAnalysis, 0, len(newest))
	for _, a := range newest {
		picked = append(picked, a)
	}
	slices.SortFunc(picked, func(a, b codeQLAnalysis) int { return cmp.Compare(a.ID, b.ID) })
	return picked, nil
}
