package main

import (
	"reflect"
	"testing"
)

const changelog = `# Changelog

## [0.56.0] - 2026-10-30

### Added

- Something mentioning CVE-2020-1234 outside the security section

### Security

- Fix a path leak (GHSA-abcd-1234-wxyz, CVE-2026-12345)
- Same fix, again CVE-2026-12345

### Fixed

- A bug

## [0.55.0] - 2026-10-22

### Fixed

- Things
`

func TestSecurityContent(t *testing.T) {
	has, ids, err := securityContent(changelog, "0.56.0")
	if err != nil || !has || !reflect.DeepEqual(ids, []string{"GHSA-abcd-1234-wxyz", "CVE-2026-12345"}) {
		t.Errorf("0.56.0: got %v %v %v", has, ids, err)
	}
	has, ids, err = securityContent(changelog, "0.55.0")
	if err != nil || has || ids != nil {
		t.Errorf("0.55.0: got %v %v %v", has, ids, err)
	}
	if _, _, err := securityContent(changelog, "0.57.0"); err == nil {
		t.Error("a missing version should fail")
	}
}
