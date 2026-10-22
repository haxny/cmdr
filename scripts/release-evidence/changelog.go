package main

import (
	"fmt"
	"regexp"
	"slices"
	"strings"
)

var advisoryIDRE = regexp.MustCompile(`\b(GHSA(?:-[0-9a-z]{4}){3}|CVE-\d{4}-\d{4,})\b`)

// securityContent reads the version's CHANGELOG section: whether it has a `### Security`
// subsection, and the advisory and CVE ids that subsection names.
func securityContent(changelog, version string) (hasSection bool, ids []string, err error) {
	section, ok := versionSection(changelog, version)
	if !ok {
		return false, nil, fmt.Errorf("CHANGELOG.md at the tag has no `## [%s]` section", version)
	}
	var security []string
	inSecurity := false
	for line := range strings.SplitSeq(section, "\n") {
		if strings.HasPrefix(line, "### ") {
			inSecurity = strings.TrimSpace(line) == "### Security"
			hasSection = hasSection || inSecurity
			continue
		}
		if inSecurity {
			security = append(security, line)
		}
	}
	for _, id := range advisoryIDRE.FindAllString(strings.Join(security, "\n"), -1) {
		if !slices.Contains(ids, id) {
			ids = append(ids, id)
		}
	}
	return hasSection, ids, nil
}

// versionSection is the text under `## [version]`, up to the next `## ` heading.
func versionSection(changelog, version string) (string, bool) {
	var lines []string
	in := false
	for line := range strings.SplitSeq(changelog, "\n") {
		if strings.HasPrefix(line, "## ") {
			if in {
				break
			}
			in = strings.HasPrefix(line, "## ["+version+"]")
			continue
		}
		if in {
			lines = append(lines, line)
		}
	}
	return strings.Join(lines, "\n"), in
}
