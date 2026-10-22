package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
)

// The company archive is an SMB folder reached with the `smb2` CLI. Its address comes from
// `-archive` or $CMDR_RELEASE_EVIDENCE_ARCHIVE, and the login from smb2's own $SMB2_USER and
// $SMB2_PASS_COMMAND: no host, share, or path lives in this public repo. The values: the vault's
// CRA release log note, which `docs/guides/releasing.md` points to.
const archiveEnv = "CMDR_RELEASE_EVIDENCE_ARCHIVE"

type fileEntry struct {
	Name        string `json:"name"`
	Size        int64  `json:"size"`
	IsDirectory bool   `json:"isDirectory"`
}

// archiveFolder checks an `//host/share/path` target and returns the release's folder in it.
func archiveFolder(target, tag string) (string, error) {
	t := strings.TrimSuffix(target, "/")
	parts := strings.Split(strings.TrimPrefix(t, "//"), "/")
	if !strings.HasPrefix(t, "//") || len(parts) < 3 || slices.Contains(parts, "") || slices.Contains(parts, "..") {
		return "", fmt.Errorf("the archive %q isn't `//host/share/path/to/folder`", target)
	}
	return t + "/" + tag, nil
}

// pathInShare drops `//host/share/` from a target: what the release log names.
func pathInShare(target string) string {
	parts := strings.SplitN(strings.TrimPrefix(target, "//"), "/", 3)
	if len(parts) < 3 {
		return target
	}
	return parts[2]
}

func localFiles(dir string) ([]fileEntry, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	var files []fileEntry
	for _, e := range entries {
		info, err := e.Info()
		if err != nil {
			return nil, err
		}
		if !info.Mode().IsRegular() {
			return nil, fmt.Errorf("%s isn't a regular file", filepath.Join(dir, e.Name()))
		}
		files = append(files, fileEntry{Name: e.Name(), Size: info.Size()})
	}
	return files, nil
}

// planUpload is the local files the archive folder doesn't hold yet. A file already there with
// the same size counts as uploaded, so a re-run after an interruption resumes. Anything else in
// the folder (a different size, a file this run didn't collect, a subfolder) stops it: the
// archive keeps evidence for 10 years, and this never overwrites or mixes it.
func planUpload(local, remote []fileEntry) ([]fileEntry, error) {
	byName := map[string]fileEntry{}
	for _, f := range local {
		byName[f.Name] = f
	}
	have := map[string]bool{}
	var conflicts []string
	for _, r := range remote {
		l, ok := byName[r.Name]
		switch {
		case r.IsDirectory:
			conflicts = append(conflicts, r.Name+"/ is a folder")
		case !ok:
			conflicts = append(conflicts, r.Name+" isn't in this run's evidence")
		case l.Size != r.Size:
			conflicts = append(conflicts, fmt.Sprintf("%s is %d bytes there and %d here", r.Name, r.Size, l.Size))
		default:
			have[r.Name] = true
		}
	}
	if len(conflicts) > 0 {
		slices.Sort(conflicts)
		return nil, fmt.Errorf("the archive folder already holds other evidence, so nothing was uploaded:\n  - %s", strings.Join(conflicts, "\n  - "))
	}
	var todo []fileEntry
	for _, f := range local {
		if !have[f.Name] {
			todo = append(todo, f)
		}
	}
	return todo, nil
}

// compareListing requires the archive folder to hold exactly the local files, size for size.
func compareListing(local, remote []fileEntry) error {
	todo, err := planUpload(local, remote)
	if err != nil {
		return err
	}
	if len(todo) > 0 {
		names := make([]string, len(todo))
		for i, f := range todo {
			names[i] = f.Name
		}
		return fmt.Errorf("the archive folder is missing %s after the upload", strings.Join(names, ", "))
	}
	return nil
}

func listArchive(folder string) ([]fileEntry, error) {
	out, err := output("", "smb2", "ls", "--json", folder)
	if err != nil {
		return nil, err
	}
	var entries []fileEntry
	if err := json.Unmarshal([]byte(out), &entries); err != nil {
		return nil, fmt.Errorf("`smb2 ls --json` didn't print JSON: %w", err)
	}
	return entries, nil
}

// upload copies the evidence folder into the archive and lists it back.
func upload(dir, folder string) (int, error) {
	// smb2 would prompt for what's missing, and nobody answers a prompt in an agent's shell.
	if os.Getenv("SMB2_USER") == "" || os.Getenv("SMB2_PASS_COMMAND")+os.Getenv("SMB2_PASS") == "" {
		return 0, errors.New("set $SMB2_USER and $SMB2_PASS_COMMAND for the archive's login (the CRA release log note has them)")
	}
	local, err := localFiles(dir)
	if err != nil {
		return 0, err
	}
	if _, err := output("", "smb2", "mkdir", "-p", folder); err != nil {
		return 0, err
	}
	remote, err := listArchive(folder)
	if err != nil {
		return 0, err
	}
	todo, err := planUpload(local, remote)
	if err != nil {
		return 0, err
	}
	if len(todo) < len(local) {
		ok("%d of %d files are already in the archive with the same size; uploading the other %d", len(local)-len(todo), len(local), len(todo))
	}
	for _, f := range todo {
		if _, err := output("", "smb2", "put", filepath.Join(dir, f.Name), folder+"/"+f.Name); err != nil {
			return 0, err
		}
	}
	back, err := listArchive(folder)
	if err != nil {
		return 0, err
	}
	if err := compareListing(local, back); err != nil {
		return 0, err
	}
	return len(local), nil
}
