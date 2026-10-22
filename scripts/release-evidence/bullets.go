package main

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

// releaseFacts is what the release log's evidence sub-bullets say. The note's header defines
// each bullet; v0.54.0's and v0.55.0's entries are worked examples.
type releaseFacts struct {
	repo, version, tag string
	taggedCommit       string
	ci                 ciCandidate
	ciRun, codeQLRun   workflowRun
	releaseRuns        []workflowRun
	lanes              []testLane
	oldMacOSSmoke      bool
	hasSecurity        bool
	advisoryIDs        []string
	archivePath        string // the folder inside the share, or "" when nothing was uploaded
	archiveFiles       int
}

// releaseLogBullets renders the sub-bullets, indented to sit under the release's line.
func releaseLogBullets(f releaseFacts) string {
	bullets := []string{
		fmt.Sprintf("**CI**: full `ci.yml` run %d (%s) on `%s`, %s; `codeql.yml` run %d (%s) on the same commit.",
			f.ciRun.ID, f.ciRun.Conclusion, f.ci.commit, f.ci.describe, f.codeQLRun.ID, f.codeQLRun.Conclusion),
		fmt.Sprintf("**Release runs**: `release.yml` %s; tagged commit `%s`.", releaseRunsText(f.releaseRuns), f.taggedCommit),
		fmt.Sprintf("**SBOMs and provenance**: https://github.com/%s/releases/tag/%s (three CycloneDX SBOMs and `Cmdr_%s.intoto.jsonl`); `gh attestation verify` on `Cmdr_%s_aarch64.dmg` passed for provenance (signer `release-pipeline.yml` at `refs/tags/%s`) and the SBOM binding.",
			f.repo, f.tag, f.version, f.version, f.tag),
		"**Test evidence**: " + testEvidenceText(f.lanes, f.oldMacOSSmoke),
		"**Security content**: " + securityText(f.hasSecurity, f.advisoryIDs),
		"**NAS copy**: " + archiveText(f.archivePath, f.archiveFiles),
	}
	var b strings.Builder
	for _, line := range bullets {
		b.WriteString("    - " + line + "\n")
	}
	return b.String()
}

func releaseRunsText(runs []workflowRun) string {
	parts := make([]string, len(runs))
	for i, r := range runs {
		parts[i] = fmt.Sprintf("%d (%s, %s)", r.ID, eventLabel(r.Event), r.Conclusion)
	}
	return strings.Join(parts, " and ")
}

// summaryLimit keeps a warn lane's reason readable in a bullet; the full text is in the log.
const summaryLimit = 160

func testEvidenceText(lanes []testLane, oldMacOSSmoke bool) string {
	parts := make([]string, len(lanes))
	for i, l := range lanes {
		summary := withThousands(l.summary)
		if summary == "" {
			summary = "passed"
		}
		if r := []rune(summary); len(r) > summaryLimit {
			summary = string(r[:summaryLimit]) + "…"
		}
		status := ""
		if l.status != "OK" {
			status = ", " + l.status
		}
		parts[i] = fmt.Sprintf("`%s` (%s%s): %s", l.name, l.job, status, summary)
	}
	smoke := "The old-macOS smoke test didn't run."
	if oldMacOSSmoke {
		smoke = "The old-macOS smoke test ran."
	}
	if len(parts) == 0 {
		return "no test lane results found in the CI log. " + smoke
	}
	return strings.Join(parts, "; ") + ". " + smoke
}

func securityText(has bool, ids []string) string {
	if !has {
		return "the CHANGELOG has no `### Security` section."
	}
	if len(ids) == 0 {
		return "the CHANGELOG has a `### Security` section; no advisory or CVE ids."
	}
	return "the CHANGELOG has a `### Security` section, naming " + strings.Join(ids, ", ") + "."
}

func archiveText(path string, files int) string {
	if path == "" {
		return "not uploaded yet (run `release-evidence.sh evidence` with `-archive`)."
	}
	return fmt.Sprintf("`%s/` (%d files, listed back after upload).", path, files)
}

var bigCountRE = regexp.MustCompile(`\b\d{4,}\b`)

// withThousands puts thousands separators in a summary's counts ("17121 tests" → "17,121 tests").
func withThousands(s string) string {
	return bigCountRE.ReplaceAllStringFunc(s, func(digits string) string {
		n, err := strconv.ParseInt(digits, 10, 64)
		if err != nil {
			return digits
		}
		return groupThousands(n)
	})
}

func groupThousands(n int64) string {
	s := strconv.FormatInt(n, 10)
	var b strings.Builder
	for i, c := range s {
		if i > 0 && (len(s)-i)%3 == 0 {
			b.WriteByte(',')
		}
		b.WriteRune(c)
	}
	return b.String()
}
