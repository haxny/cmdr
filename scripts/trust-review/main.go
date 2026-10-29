// Trust review: tells the releasing agent which sections of getcmdr.com/trust need a fresh look,
// so the page stays true of every release without rereading all of it each time
// (`docs/guides/trust-page-review.md`).
//
// The source map, `apps/website/src/lib/trust-sources.json`, lists for each `<h2 id>` of the trust
// pages the repo paths whose changes could make that section's claims wrong. This tool:
//
//   - `changes` (the default): reads `verifiedAgainst` in `apps/website/src/lib/trust.ts`, diffs
//     its tag (or `-since`) against `-to` (default HEAD), and prints each section whose paths
//     changed, with the commits that touched them. Sections with no change are listed by name only.
//   - `validate`: fails when the map and the pages drift: a heading with no entry, an entry for a
//     heading that's gone, or a path pattern that matches no tracked file. The
//     `website-trust-sources` check runs it.
//
// # Usage
//
//	./scripts/trust-review.sh
//	./scripts/trust-review.sh changes -since v0.55.0 -to v0.57.0
//	./scripts/trust-review.sh validate
package main

import (
	"flag"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

func main() {
	root := flag.String("root", "", "the repo root (scripts/trust-review.sh sets it)")
	flag.Usage = usage
	flag.Parse()
	if *root == "" {
		usage()
		os.Exit(2)
	}
	sub := "changes"
	args := flag.Args()
	if len(args) > 0 {
		sub, args = args[0], args[1:]
	}
	var err error
	switch sub {
	case "changes":
		err = runChanges(*root, args)
	case "validate":
		err = runValidate(*root)
	default:
		usage()
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "trust-review:", err)
		os.Exit(1)
	}
}

func usage() {
	fmt.Fprint(os.Stderr, `Usage:
  ./scripts/trust-review.sh [changes] [-since <ref>] [-to <ref>]
  ./scripts/trust-review.sh validate
`)
}

func runChanges(root string, args []string) error {
	fs := flag.NewFlagSet("changes", flag.ExitOnError)
	since := fs.String("since", "", "the ref the page was last verified against (default: v<verifiedAgainst.version>)")
	to := fs.String("to", "HEAD", "the ref to review up to")
	_ = fs.Parse(args)

	verified, err := readVerifiedAgainst(root)
	if err != nil {
		return err
	}
	base := *since
	if base == "" {
		base = "v" + verified.version
	}
	m, err := loadSourceMap(root)
	if err != nil {
		return err
	}
	changedOut, err := git(root, "diff", "--name-only", base+".."+*to)
	if err != nil {
		return err
	}
	changed := nonEmptyLines(changedOut)

	fmt.Printf("Trust pages last verified against %s on %s. Reviewing %s..%s (%d changed files).\n\n",
		verified.version, verified.date, base, *to, len(changed))
	var touched, untouched []section
	for _, s := range m.Sections {
		if len(s.matching(changed)) > 0 {
			touched = append(touched, s)
		} else {
			untouched = append(untouched, s)
		}
	}
	fmt.Printf("Recheck %d of %d sections:\n", len(touched), len(m.Sections))
	for _, s := range touched {
		files := s.matching(changed)
		fmt.Printf("\n## %s#%s\n", s.Page, s.Anchor)
		fmt.Printf("   %d changed %s: %s\n", len(files), plural(len(files), "file", "files"), capList(files, 8))
		logArgs := append([]string{"log", "--no-merges", "--format=%h %s", base + ".." + *to, "--"}, files...)
		commits, err := git(root, logArgs...)
		if err != nil {
			return err
		}
		lines := nonEmptyLines(commits)
		for i, c := range lines {
			if i == 12 {
				fmt.Printf("   ... and %d more commits\n", len(lines)-12)
				break
			}
			fmt.Printf("   - %s\n", c)
		}
	}
	if len(untouched) > 0 {
		names := make([]string, len(untouched))
		for i, s := range untouched {
			names[i] = s.Page + "#" + s.Anchor
		}
		fmt.Printf("\nNo source changes (skip unless the CHANGELOG says otherwise): %s\n", strings.Join(names, ", "))
	}
	return nil
}

func runValidate(root string) error {
	m, err := loadSourceMap(root)
	if err != nil {
		return err
	}
	tracked, err := git(root, "ls-files")
	if err != nil {
		return err
	}
	problems, err := m.validate(root, nonEmptyLines(tracked))
	if err != nil {
		return err
	}
	if len(problems) > 0 {
		return fmt.Errorf("the trust source map (%s) is out of date:\n  %s", sourceMapPath, strings.Join(problems, "\n  "))
	}
	fmt.Printf("%d sections mapped, every pattern matches.\n", len(m.Sections))
	return nil
}

func git(root string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = root
	out, err := cmd.Output()
	if err != nil {
		if ee, ok := err.(*exec.ExitError); ok {
			return "", fmt.Errorf("git %s: %w\n%s", strings.Join(args, " "), err, ee.Stderr)
		}
		return "", fmt.Errorf("git %s: %w", strings.Join(args, " "), err)
	}
	return string(out), nil
}

func nonEmptyLines(s string) []string {
	var lines []string
	for line := range strings.SplitSeq(s, "\n") {
		if line = strings.TrimSpace(line); line != "" {
			lines = append(lines, line)
		}
	}
	return lines
}

func capList(items []string, limit int) string {
	if len(items) <= limit {
		return strings.Join(items, ", ")
	}
	return strings.Join(items[:limit], ", ") + fmt.Sprintf(", and %d more", len(items)-limit)
}

func plural(n int, one, many string) string {
	if n == 1 {
		return one
	}
	return many
}
