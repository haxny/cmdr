package main

import (
	"bytes"
	"fmt"
	"os/exec"
	"path/filepath"
)

// runner runs a command in a directory and answers its combined output, so tests can stand in
// for cargo.
type runner func(dir, name string, args ...string) (string, error)

// vetArchives refuses to go on unless every archive's bundle passes the check the macOS updater
// runs before installing: unpacked with the installer's own extraction, code-signed by Cmdr's
// Developer ID, and naming this release's version. It runs the app's own code
// (`crates/cmdr-update-trust`, binary `cmdr-vet-update`), so the gate and the updater can't drift.
// Without it, a bundle the updater refuses could be signed and published, and every install would
// be stuck on its version with no update able to reach it.
func vetArchives(run runner, root, version, dir string, names []string) error {
	step("Checking each bundle's code signature the way the updater does")
	args := []string{"run", "--quiet", "--release", "--locked", "-p", "cmdr-update-trust", "--bin", "cmdr-vet-update", "--",
		"--version", version}
	for _, name := range names {
		args = append(args, filepath.Join(dir, name))
	}
	out, err := run(root, "cargo", args...)
	fmt.Println(out)
	if err != nil {
		return fmt.Errorf("refusing to sign: the updater would refuse at least one of these archives (above), so no install could take this release: %w", err)
	}
	return nil
}

// runCombined is the real runner.
func runCombined(dir, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &out
	err := cmd.Run()
	return string(bytes.TrimRight(out.Bytes(), "\n")), err
}
