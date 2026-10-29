package main

import (
	"bufio"
	"fmt"
	"io"
	"os/exec"
	"strconv"
	"strings"
)

type commit struct {
	hash     string
	date     string
	messages []string
}

// repoRoot caches the git top-level directory so all commands run from the repo root.
var repoRoot string

func findRepoRoot() (string, error) {
	if repoRoot != "" {
		return repoRoot, nil
	}
	cmd := exec.Command("git", "rev-parse", "--show-toplevel")
	output, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("failed to find git repo root: %w", err)
	}
	repoRoot = strings.TrimSpace(string(output))
	return repoRoot, nil
}

func gitCommand(args ...string) *exec.Cmd {
	root, err := findRepoRoot()
	if err != nil {
		// Fall back to running without -C
		return exec.Command("git", args...)
	}
	fullArgs := append([]string{"-C", root}, args...)
	return exec.Command("git", fullArgs...)
}

// getCommits lists main's commits, newest first. firstParent: only main's own tips, skipping the
// commits a merge brought in, so each one is a state `main` was really in. Otherwise every commit
// but the merges themselves, for the messages.
func getCommits(firstParent bool) ([]commit, error) {
	args := []string{"log", "--format=%H|%cd|%s", "--date=short"}
	if firstParent {
		args = append(args, "--first-parent")
	} else {
		args = append(args, "--no-merges")
	}
	cmd := gitCommand(append(args, "main")...)
	output, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("failed to run git log: %w", err)
	}

	var commits []commit
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}

		parts := strings.SplitN(line, "|", 3)
		if len(parts) != 3 {
			continue
		}

		commits = append(commits, commit{
			hash:     parts[0],
			date:     parts[1],
			messages: []string{parts[2]},
		})
	}

	return commits, scanner.Err()
}

// groupCommitsByDate picks, per day, the state `main` was in at its end and that day's commit
// messages. tips is `main`'s first-parent history and all its non-merge commits, both newest
// first. A merge commit brings in branch commits dated before it, so a day can have messages but
// no tip of its own; it takes the latest earlier tip, since `main` didn't change that day.
func groupCommitsByDate(tips, all []commit) map[string]commit {
	daily := make(map[string]commit)
	for _, c := range tips {
		if _, ok := daily[c.date]; !ok {
			// Newest first, so the first tip of a date is that day's last.
			daily[c.date] = commit{hash: c.hash, date: c.date}
		}
	}
	for _, c := range all {
		day, ok := daily[c.date]
		if !ok {
			// ISO dates sort as strings; tips run newest first, so the first older one is the latest.
			for _, t := range tips {
				if t.date < c.date {
					day, ok = commit{hash: t.hash, date: c.date}, true
					break
				}
			}
			if !ok {
				continue
			}
		}
		day.messages = append(day.messages, c.messages...)
		daily[c.date] = day
	}
	return daily
}

type fileEntry struct {
	path string
	blob string // blob SHA for use with cat-file --batch
}

func getFilesAtCommit(commitHash string) ([]fileEntry, error) {
	cmd := gitCommand("ls-tree", "-r", commitHash)
	output, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("failed to run git ls-tree: %w", err)
	}

	var files []fileEntry
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := scanner.Text()
		if line == "" {
			continue
		}
		// Format: "<mode> <type> <hash>\t<path>"
		tabIdx := strings.IndexByte(line, '\t')
		if tabIdx < 0 {
			continue
		}
		meta := line[:tabIdx]
		path := line[tabIdx+1:]
		parts := strings.Fields(meta)
		if len(parts) < 3 || parts[1] != "blob" {
			continue // skip trees/submodules
		}
		files = append(files, fileEntry{path: path, blob: parts[2]})
	}

	return files, scanner.Err()
}

// batchGetFileContents fetches all blob contents in a single git cat-file --batch process.
// Returns a map from blob hash to file content. Blobs that fail to read (binary/missing) are omitted.
func batchGetFileContents(blobs []string) (map[string]string, error) {
	if len(blobs) == 0 {
		return nil, nil
	}

	root, err := findRepoRoot()
	if err != nil {
		return nil, err
	}

	cmd := exec.Command("git", "-C", root, "cat-file", "--batch")
	cmd.Stderr = nil

	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("failed to create stdin pipe: %w", err)
	}

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("failed to create stdout pipe: %w", err)
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("failed to start cat-file: %w", err)
	}

	// Write all blob hashes to stdin in a goroutine to avoid deadlock
	go func() {
		for _, blob := range blobs {
			fmt.Fprintln(stdin, blob)
		}
		stdin.Close()
	}()

	// Read responses: each is "<hash> <type> <size>\n<content of size bytes>\n"
	// or "<hash> missing\n"
	reader := bufio.NewReaderSize(stdout, 256*1024)
	contents := make(map[string]string, len(blobs))

	for range blobs {
		header, err := reader.ReadString('\n')
		if err != nil {
			break
		}
		header = strings.TrimRight(header, "\n")

		if strings.HasSuffix(header, " missing") {
			continue
		}

		fields := strings.Fields(header)
		if len(fields) < 3 {
			continue
		}

		hash := fields[0]
		size, err := strconv.Atoi(fields[2])
		if err != nil {
			continue
		}

		// Read exactly 'size' bytes of content + trailing newline
		buf := make([]byte, size+1) // +1 for the trailing LF
		if _, err := io.ReadFull(reader, buf); err != nil {
			break
		}

		content := string(buf[:size])
		// Skip likely-binary content (contains null bytes in the first chunk)
		if strings.ContainsRune(content[:min(len(content), 8000)], '\x00') {
			continue
		}
		contents[hash] = content
	}

	// Drain and wait; ignore exit errors (broken pipe if we stop reading early)
	_ = cmd.Wait()

	return contents, nil
}

func countLines(content string) int {
	lines := strings.Count(content, "\n")
	if len(content) > 0 && !strings.HasSuffix(content, "\n") {
		lines++
	}
	return lines
}
