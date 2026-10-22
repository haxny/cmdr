package main

import (
	"bufio"
	"io"
	"regexp"
	"strings"
)

// testLane is one test lane's result in a CI log: a check-runner test lane, or the Playwright
// suite, which the CI job runs outside the check runner.
type testLane struct {
	job     string // the CI job that ran it
	name    string // the check name, or "playwright"
	status  string // the check runner's word: OK, warn, ...
	summary string
}

var (
	// GitHub serves run logs with color codes as caret text (`^[[32m`), not the ESC byte.
	ansiRE = regexp.MustCompile(`(?:\x1b|\^\[)\[[0-9;]*m`)
	// `gh run view --log` lines: `<job>\t<step>\t<timestamp> <text>`.
	logLineRE = regexp.MustCompile(`^([^\t]*)\t[^\t]*\t\S+Z (.*)$`)
	// A check-runner result: `• 🖥️  Desktop: 🦀 Rust / rust-tests... OK (11m38s) - 13044 tests passed`.
	checkLineRE = regexp.MustCompile(`^• .+? / ([A-Za-z0-9_-]+)\.\.\. (\S+)(?: \([^)]*\))?(?: - (.*))?$`)
	// Playwright's closing tally: `  40 skipped`, `  416 passed (8.7m)`.
	playwrightTallyRE = regexp.MustCompile(`^\s+(\d+ (?:passed|failed|flaky|skipped|did not run|interrupted))(?: \([^)]*\))?$`)
)

// isTestLane says whether a check-runner check runs tests (as opposed to a linter or scanner):
// the `*tests` lanes, the `*-e2e` lanes, and the WebDAV interop cells.
func isTestLane(check string) bool {
	return strings.HasSuffix(check, "tests") || strings.HasSuffix(check, "-e2e") || strings.HasPrefix(check, "webdav-")
}

// parseTestLanes reads a `gh run view --log` CI log and returns each test lane's result in log
// order. A lane whose summary isn't on its result line takes its first indented detail line.
func parseTestLanes(r io.Reader) ([]testLane, error) {
	lanes := make([]testLane, 0, 16)
	awaiting := map[string]int{} // job -> index of a lane still waiting for its detail line
	tallies := map[string][]string{}
	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 1<<20), 16<<20)
	for sc.Scan() {
		m := logLineRE.FindStringSubmatch(ansiRE.ReplaceAllString(sc.Text(), ""))
		if m == nil {
			continue
		}
		job, text := m[1], m[2]
		if i, ok := awaiting[job]; ok {
			delete(awaiting, job)
			if detail := strings.TrimSpace(text); strings.HasPrefix(text, "    ") && detail != "" {
				lanes[i].summary = detail
			}
		}
		if c := checkLineRE.FindStringSubmatch(text); c != nil {
			if isTestLane(c[1]) {
				lanes = append(lanes, testLane{job: job, name: c[1], status: c[2], summary: c[3]})
				if c[3] == "" {
					awaiting[job] = len(lanes) - 1
				}
			}
			continue
		}
		if t := playwrightTallyRE.FindStringSubmatch(text); t != nil {
			tallies[job] = append(tallies[job], t[1])
			if strings.HasSuffix(t[1], " passed") {
				lanes = append(lanes, testLane{job: job, name: "playwright", status: "OK", summary: strings.Join(tallies[job], ", ")})
				delete(tallies, job)
			}
		}
	}
	return lanes, sc.Err()
}
