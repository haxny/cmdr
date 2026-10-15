package checks

import (
	"errors"
	"strings"
	"testing"
)

func TestScreenshotsCheckResult_CleanRunIsSuccess(t *testing.T) {
	result, err := screenshotsCheckResult(0, "")
	if err != nil {
		t.Fatalf("got error %v, want success", err)
	}
	if result.Code != ResultSuccess {
		t.Fatalf("got code %d, want success", result.Code)
	}
}

func TestScreenshotsCheckResult_DriftIsAWarnWithEachFindingCounted(t *testing.T) {
	summary := "2 stale couplings (`pnpm i18n:couple` rewrites them), and 1 key the capture report still names but the catalogs dropped"
	output := summary + "\n" +
		"Stale screenshot couplings (2):\n" +
		"  - common.ok → dialog.png (currently undefined)\n" +
		"  - common.cancel → no screenshot (stale coupling to clear) (currently \"old.png\")\n" +
		"Keys the capture report names but the catalogs dropped (1):\n" +
		"  - common.gone (not in common.json)\n"
	result, err := screenshotsCheckResult(screenshotsExitWarn, output)
	if err != nil {
		t.Fatalf("got error %v, want a warn", err)
	}
	if result.Code != ResultWarning {
		t.Fatalf("got code %d, want warning", result.Code)
	}
	if result.Issues != 3 {
		t.Fatalf("got %d issues, want 3", result.Issues)
	}
	// Quiet stdout gets the summary alone; the list goes to the saved output.
	if result.Message != summary {
		t.Fatalf("got message %q, want the coupler's one-line summary", result.Message)
	}
	if !strings.Contains(result.Details, "common.gone") || strings.Contains(result.Details, summary) {
		t.Fatalf("got details %q, want the bulleted sections without the summary", result.Details)
	}
}

func TestScreenshotsCheckResult_StructuralBreakFails(t *testing.T) {
	output := "Broken screenshot rules (1):\n  - `fileExplorer.smbReconnect.` matches no catalog key\n"
	_, err := screenshotsCheckResult(screenshotsExitError, output)
	if err == nil {
		t.Fatal("got no error, want the structural break to fail the check")
	}
	if errors.Is(err, errScreenshotCouplerAbnormalExit) {
		t.Fatal("a structural break must read as a finding, not as the coupler crashing")
	}
}

func TestScreenshotsCheckResult_CrashIsNeverAWarn(t *testing.T) {
	// Node exits 1 on an uncaught exception. That was once the coupler's own drift
	// code, so a crash passed as a warn; it has to fail loudly instead.
	result, err := screenshotsCheckResult(1, "TypeError: cannot read properties of undefined\n")
	if !errors.Is(err, errScreenshotCouplerAbnormalExit) {
		t.Fatalf("got error %v, want the abnormal-exit error", err)
	}
	if result.Code == ResultWarning {
		t.Fatal("a crash must never be reported as a warn")
	}
}

func TestCountDriftLines_CountsBulletedCouplings(t *testing.T) {
	output := "Missing/stale screenshot couplings (2):\n" +
		"  - common.ok → dialog.png (currently undefined)\n" +
		"  - common.cancel → dialog.png (currently \"old.png\")\n"
	if got := countDriftLines(output); got != 2 {
		t.Fatalf("got %d drift lines, want 2", got)
	}
}

func TestCountDriftLines_ZeroWhenNoBullets(t *testing.T) {
	output := "All captured keys are already coupled to their screenshots.\n"
	if got := countDriftLines(output); got != 0 {
		t.Fatalf("got %d drift lines, want 0", got)
	}
}

func TestCountDriftLines_IgnoresHeaderLine(t *testing.T) {
	// The "(N):" header must NOT be counted as a drift line; only the "  - …"
	// bullets are. A regression here would double-count or miscount.
	output := "Missing/stale screenshot couplings (1):\n  - settings.x → s.png (currently undefined)\n"
	if got := countDriftLines(output); got != 1 {
		t.Fatalf("got %d drift lines, want 1", got)
	}
}
