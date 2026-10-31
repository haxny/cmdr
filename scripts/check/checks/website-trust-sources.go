package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunWebsiteTrustSources fails when `apps/website/src/lib/trust-sources.json` drifts from the
// trust pages: a section heading with no entry, an entry whose heading is gone, or a path pattern
// that no longer matches any tracked file (a renamed module). The per-release trust page review
// reads that map to decide which sections to recheck, so a stale map silently skips a section
// whose claims went wrong (`docs/guides/trust-page-review.md`). The logic lives in the review tool
// itself, `scripts/trust-review/`, so the check and the tool can't disagree.
func RunWebsiteTrustSources(ctx *CheckContext) (CheckResult, error) {
	cmd := exec.Command("go", "run", ".", "-root", ctx.RootDir, "validate")
	cmd.Dir = filepath.Join(ctx.RootDir, "scripts", "trust-review")
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("trust source map out of date\n%s", indentOutput(strings.TrimSpace(output)))
	}
	return Success(strings.TrimSpace(output)), nil
}
