package checks

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// securityAdvisoriesRelPath is the committed advisory list the app bundles and embeds.
const securityAdvisoriesRelPath = "apps/desktop/src-tauri/resources/security-advisories.json"

// RunSecurityAdvisories validates the committed security advisory list against the
// same contract the app's parser and link allowlist hold it to
// (`apps/desktop/src-tauri/src/security_advisories/`).
//
// The app is lenient on purpose: a list that doesn't parse is "no advisory", because
// the update carrying it is the fix itself. That leniency is exactly why the list has
// to be right before it ships, since a typo would silently drop a notice users should
// act on. So this fails on anything the app would refuse or quietly ignore, plus two
// things only a release can get wrong: a `fixedIn` past the app version, and a
// reused `id`.
func RunSecurityAdvisories(ctx *CheckContext) (CheckResult, error) {
	data, err := os.ReadFile(filepath.Join(ctx.RootDir, securityAdvisoriesRelPath))
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", securityAdvisoriesRelPath, err)
	}
	appVersion, err := readTauriConfVersion(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	if problems := securityAdvisoryProblems(data, appVersion); len(problems) > 0 {
		return CheckResult{}, fmt.Errorf("%s:\n%s", securityAdvisoriesRelPath, indentOutput(strings.Join(problems, "\n")))
	}
	var file securityAdvisoryFile
	_ = json.Unmarshal(data, &file) // Already validated above.
	count := len(file.Advisories)
	return Success(fmt.Sprintf("%d %s valid against v%s", count, Pluralize(count, "advisory", "advisories"), appVersion)), nil
}

// readTauriConfVersion is the app version every release stamps into `tauri.conf.json`.
func readTauriConfVersion(rootDir string) (string, error) {
	data, err := os.ReadFile(filepath.Join(rootDir, tauriConfRelPath))
	if err != nil {
		return "", fmt.Errorf("couldn't read %s: %w", tauriConfRelPath, err)
	}
	var conf struct {
		Version string `json:"version"`
	}
	if err := json.Unmarshal(data, &conf); err != nil || conf.Version == "" {
		return "", fmt.Errorf("couldn't read the app version from %s", tauriConfRelPath)
	}
	return conf.Version, nil
}

type securityAdvisoryFile struct {
	Advisories []securityAdvisoryEntry `json:"advisories"`
}

// securityAdvisoryEntry mirrors the app's `RawAdvisory`. Pointers tell a missing
// field from an empty one.
type securityAdvisoryEntry struct {
	ID       *string `json:"id"`
	FixedIn  *string `json:"fixedIn"`
	Severity *string `json:"severity"`
	URL      *string `json:"url"`
}

// securitySeverities are the CVSS v4.0 ratings, spelled as the app's `Severity` serializes them.
var securitySeverities = map[string]bool{"low": true, "medium": true, "high": true, "critical": true}

// securityAdvisoryProblems lists everything wrong with an advisory list, judged
// against `appVersion`. Empty means valid.
func securityAdvisoryProblems(data []byte, appVersion string) []string {
	var file securityAdvisoryFile
	decoder := json.NewDecoder(bytes.NewReader(data))
	// The app's parser has `deny_unknown_fields` at both levels, so an unknown key
	// there means the whole list reads as "no advisory".
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&file); err != nil {
		return []string{fmt.Sprintf("doesn't parse as `{\"advisories\": [{id, fixedIn, severity, url}, ...]}`, so the app would show no advisory at all: %v", err)}
	}
	app, ok := parseStrictSemver(appVersion)
	if !ok {
		return []string{fmt.Sprintf("the app version %q isn't semver", appVersion)}
	}

	var problems []string
	seen := map[string]bool{}
	for i, entry := range file.Advisories {
		label := fmt.Sprintf("advisory %d", i+1)
		if entry.ID != nil && *entry.ID != "" {
			label = fmt.Sprintf("advisory %q", *entry.ID)
		}
		for _, field := range []struct {
			name  string
			value *string
		}{{"id", entry.ID}, {"fixedIn", entry.FixedIn}, {"severity", entry.Severity}, {"url", entry.URL}} {
			if field.value == nil || *field.value == "" {
				problems = append(problems, fmt.Sprintf("%s: `%s` is missing or empty", label, field.name))
			}
		}
		if entry.ID != nil && *entry.ID != "" {
			if seen[*entry.ID] {
				problems = append(problems, fmt.Sprintf("%s: duplicate `id`; each advisory needs its own", label))
			}
			seen[*entry.ID] = true
		}
		if entry.FixedIn != nil && *entry.FixedIn != "" {
			fixed, ok := parseStrictSemver(*entry.FixedIn)
			switch {
			case !ok:
				problems = append(problems, fmt.Sprintf("%s: `fixedIn` %q isn't semver (like `0.54.0`)", label, *entry.FixedIn))
			case fixed.compare(app) > 0:
				problems = append(problems, fmt.Sprintf("%s: `fixedIn` %s is newer than the app (v%s); an entry names the release that fixes it", label, *entry.FixedIn, appVersion))
			}
		}
		if entry.Severity != nil && *entry.Severity != "" && !securitySeverities[*entry.Severity] {
			problems = append(problems, fmt.Sprintf("%s: `severity` %q isn't one of low, medium, high, critical", label, *entry.Severity))
		}
		if entry.URL != nil && *entry.URL != "" && !isAllowedAdvisoryURL(*entry.URL) {
			problems = append(problems, fmt.Sprintf("%s: `url` %q isn't an allowed advisory link (https://github.com/vdavid/cmdr/security/advisories/<id> or https://getcmdr.com/changelog…)", label, *entry.URL))
		}
	}
	return problems
}

// isAllowedAdvisoryURL mirrors the app's `is_allowed_advisory_url`: exactly
// `https`, no userinfo or port, already canonical, and under one of the two allowed
// places. A GitHub advisory needs an id after its prefix; the changelog may be the
// page itself or anything below it, fragment included.
func isAllowedAdvisoryURL(raw string) bool {
	u, err := url.Parse(raw)
	if err != nil || u.String() != raw || u.Scheme != "https" || u.User != nil || u.Port() != "" || u.Opaque != "" {
		return false
	}
	// Go keeps `..` segments that the app's URL parser resolves, so refuse them
	// outright rather than disagree about where the link goes.
	for _, segment := range strings.Split(u.Path, "/") {
		if segment == ".." || segment == "." {
			return false
		}
	}
	switch u.Host {
	case "github.com":
		const prefix = "/vdavid/cmdr/security/advisories/"
		return strings.HasPrefix(u.Path, prefix) && len(u.Path) > len(prefix)
	case "getcmdr.com":
		return u.Path == "/changelog" || strings.HasPrefix(u.Path, "/changelog/")
	}
	return false
}

// strictSemverRE is semver 2.0.0's own grammar, minus the leading `v` the app's
// parser refuses too.
var strictSemverRE = regexp.MustCompile(`^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+[0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*)?$`)

type strictSemver struct {
	core       [3]int
	prerelease []string
}

func parseStrictSemver(raw string) (strictSemver, bool) {
	m := strictSemverRE.FindStringSubmatch(raw)
	if m == nil {
		return strictSemver{}, false
	}
	var v strictSemver
	for i := range 3 {
		n, err := strconv.Atoi(m[i+1])
		if err != nil {
			return strictSemver{}, false
		}
		v.core[i] = n
	}
	if m[4] != "" {
		v.prerelease = strings.Split(m[4], ".")
	}
	return v, true
}

// compare orders two versions by semver precedence: the core numerically, then a
// pre-release before its release, then pre-release identifiers left to right
// (numeric ones numerically and before alphanumeric ones).
func (v strictSemver) compare(other strictSemver) int {
	for i := range 3 {
		if v.core[i] != other.core[i] {
			return sign(v.core[i] - other.core[i])
		}
	}
	switch {
	case len(v.prerelease) == 0 && len(other.prerelease) == 0:
		return 0
	case len(v.prerelease) == 0:
		return 1
	case len(other.prerelease) == 0:
		return -1
	}
	for i := 0; i < len(v.prerelease) && i < len(other.prerelease); i++ {
		a, b := v.prerelease[i], other.prerelease[i]
		aNum, aErr := strconv.Atoi(a)
		bNum, bErr := strconv.Atoi(b)
		switch {
		case aErr == nil && bErr == nil:
			if aNum != bNum {
				return sign(aNum - bNum)
			}
		case aErr == nil:
			return -1
		case bErr == nil:
			return 1
		default:
			if c := strings.Compare(a, b); c != 0 {
				return c
			}
		}
	}
	return sign(len(v.prerelease) - len(other.prerelease))
}

func sign(n int) int {
	switch {
	case n < 0:
		return -1
	case n > 0:
		return 1
	}
	return 0
}
