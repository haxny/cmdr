package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
)

// rulesets creates or updates the repo's rulesets from `.github/rulesets/*.json`, matching them by
// name. Without `-apply` it only says what it would do. Rulesets with no file here (the tag one,
// "main: no force-push or deletion") are left alone.
func rulesets(r runner, args []string) error {
	fs := flag.NewFlagSet("rulesets", flag.ExitOnError)
	apply := fs.Bool("apply", false, "create or update the rulesets on GitHub (default: only print the plan)")
	if err := fs.Parse(args); err != nil {
		return err
	}
	files, err := filepath.Glob(filepath.Join(r.dir, ".github", "rulesets", "*.json"))
	if err != nil {
		return err
	}
	var existing []struct {
		ID   int64  `json:"id"`
		Name string `json:"name"`
	}
	if err := r.ghJSON(&existing, "api", fmt.Sprintf("repos/%s/rulesets", repo)); err != nil {
		return err
	}
	ids := make(map[string]int64, len(existing))
	for _, rs := range existing {
		ids[rs.Name] = rs.ID
	}
	for _, file := range files {
		name, err := rulesetName(file)
		if err != nil {
			return err
		}
		method, path := "POST", fmt.Sprintf("repos/%s/rulesets", repo)
		if id, ok := ids[name]; ok {
			method, path = "PUT", fmt.Sprintf("%s/%d", path, id)
		}
		say("%s %q from %s", map[string]string{"POST": "create", "PUT": "update"}[method], name, filepath.Base(file))
		if !*apply {
			continue
		}
		if _, err := r.gh("api", "-X", method, path, "--input", file); err != nil {
			return err
		}
	}
	if !*apply {
		say("Nothing changed; add -apply to do it.")
	}
	return nil
}

func rulesetName(file string) (string, error) {
	data, err := os.ReadFile(file)
	if err != nil {
		return "", err
	}
	var rs struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(data, &rs); err != nil || rs.Name == "" {
		return "", fmt.Errorf("%s: not a ruleset with a name: %v", file, err)
	}
	return rs.Name, nil
}
