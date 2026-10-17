// Transfer baseline: times one build scenario for docs/notes/transfer-extraction-baseline.md, the same way before and
// after the write engine became crates.
//
// Usage (from anywhere in the repo; it runs the command from the repo root):
//
//	go -C scripts/transfer-baseline run . [-n RUNS] [--edit FILE ANCHOR] [--pre 'SHELL CMD'] [--no-warmup] -- COMMAND...
//
//	-n RUNS        timed runs (default 5)
//	--edit F A     before every run (warm-up included), rewrite the first occurrence of the string literal fragment A
//	               in file F (repo-relative) to `A [bench <i>]`, so incremental compilation has to redo real work (never a
//	               `touch`). The file is restored to A on exit, even on Ctrl-C.
//	--pre CMD      untimed shell command before every run (for example `cargo clean -p cmdr`)
//	--no-warmup    skip the untimed warm-up run (use it when every run starts from a clean state anyway)
//
// Prints, per run: the 1/5/15-minute load average just before it, wall seconds, and CPU seconds (user + sys of every
// child process). CPU seconds survive a loaded machine far better than wall time (see archive-extraction-baseline.md).
// Ends with median / min / max of both. A run whose output mentions cargo's "Blocking waiting for file lock" is flagged
// LOCK-WAIT: it timed another session, not this build, so drop it and re-run.
//
// The method behind every number, and the numbers: docs/notes/transfer-extraction-baseline.md.
package main

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// options is one invocation's parsed command line.
type options struct {
	runs       int
	editFile   string
	editAnchor string
	pre        string
	warmup     bool
	command    []string
}

// runResult is one timed run of the command.
type runResult struct {
	load     string
	wall     float64
	cpu      float64
	exitCode int
}

func main() {
	opts, err := parseArgs(os.Args[1:])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	os.Exit(run(opts))
}

// parseArgs reads the same command line the shell driver took, so the note's recipes run unchanged. Hand-rolled
// rather than `flag`, because `--edit` takes two values.
func parseArgs(args []string) (options, error) {
	opts := options{runs: 5, warmup: true}
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "-n":
			if i+1 >= len(args) {
				return opts, errors.New("-n needs a run count")
			}
			n, err := strconv.Atoi(args[i+1])
			if err != nil || n < 1 {
				return opts, fmt.Errorf("-n needs a positive run count, got %q", args[i+1])
			}
			opts.runs = n
			i++
		case "--edit":
			if i+2 >= len(args) {
				return opts, errors.New("--edit needs a file and an anchor")
			}
			opts.editFile, opts.editAnchor = args[i+1], args[i+2]
			i += 2
		case "--pre":
			if i+1 >= len(args) {
				return opts, errors.New("--pre needs a shell command")
			}
			opts.pre = args[i+1]
			i++
		case "--no-warmup":
			opts.warmup = false
		case "--":
			opts.command = args[i+1:]
			if len(opts.command) == 0 {
				return opts, errors.New("no command given (put it after --)")
			}
			return opts, nil
		default:
			return opts, fmt.Errorf("unknown argument: %s", args[i])
		}
	}
	return opts, errors.New("no command given (put it after --)")
}

// run drives the warm-up and the timed runs and prints the report. It returns the process exit code.
func run(opts options) int {
	root, err := output("git", "rev-parse", "--show-toplevel")
	if err != nil {
		fmt.Fprintln(os.Stderr, "not inside the repo:", err)
		return 2
	}
	if err := os.Chdir(root); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}

	if opts.editFile != "" {
		src, err := os.ReadFile(opts.editFile)
		if err != nil || !strings.Contains(string(src), opts.editAnchor) {
			fmt.Fprintf(os.Stderr, "anchor not found in %s: %s\n", opts.editFile, opts.editAnchor)
			return 2
		}
		// Restore the file however the run ends: a normal return, or Ctrl-C / SIGTERM mid-run.
		restore := func() { _ = setMarker(opts.editFile, opts.editAnchor, "") }
		defer restore()
		signals := make(chan os.Signal, 1)
		signal.Notify(signals, os.Interrupt, syscall.SIGTERM)
		go func() {
			<-signals
			restore()
			os.Exit(130)
		}()
	}

	logFile, err := os.CreateTemp("", "transfer-baseline")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	logPath := logFile.Name()
	_ = logFile.Close()

	printHeader(opts)
	return timeRuns(opts, logPath)
}

// printHeader names what the report below it measured: when, at which commit, on which toolchain, and how.
func printHeader(opts options) {
	commit, _ := output("git", "rev-parse", "--short", "HEAD")
	rustc, _ := output("rustc", "-V")
	fmt.Printf("# %s  commit %s  %s\n", time.Now().Format("2006-01-02 15:04:05"), commit, rustc)
	fmt.Printf("# command: %s\n", strings.Join(opts.command, " "))
	if opts.editFile != "" {
		fmt.Printf("# edit: %s  \"%s\"\n", opts.editFile, opts.editAnchor)
	}
	if opts.pre != "" {
		fmt.Printf("# pre: %s\n", opts.pre)
	}
}

// timeRuns runs the warm-up, then the timed runs, and prints a row per run and the stats. It returns the exit code.
func timeRuns(opts options, logPath string) int {
	if opts.warmup {
		r, err := oneRun(opts, 0, logPath)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		fmt.Printf("warm-up  wall %.2fs  exit %d\n", r.wall, r.exitCode)
		if r.exitCode != 0 {
			fmt.Fprintf(os.Stderr, "warm-up failed; output in %s\n", logPath)
			return 1
		}
	}

	walls := make([]float64, 0, opts.runs)
	cpus := make([]float64, 0, opts.runs)
	for i := 1; i <= opts.runs; i++ {
		r, err := oneRun(opts, i, logPath)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		flag := ""
		if out, err := os.ReadFile(logPath); err == nil && strings.Contains(string(out), "Blocking waiting for file lock") {
			flag = "  LOCK-WAIT"
		}
		if r.exitCode != 0 {
			flag += fmt.Sprintf("  EXIT %d (output in %s)", r.exitCode, logPath)
		}
		fmt.Printf("run %d  load %s  wall %7.2fs  cpu %8.2fs%s\n", i, r.load, r.wall, r.cpu, flag)
		walls = append(walls, r.wall)
		cpus = append(cpus, r.cpu)
	}
	fmt.Printf("wall: %s\n", stats(walls))
	fmt.Printf("cpu:  %s\n", stats(cpus))
	_ = os.Remove(logPath)
	return 0
}

// oneRun edits, runs the untimed pre-step, then times the command once, its output going to a fresh log.
func oneRun(opts options, index int, logPath string) (runResult, error) {
	if opts.editFile != "" {
		if err := setMarker(opts.editFile, opts.editAnchor, fmt.Sprintf(" [bench %d]", index)); err != nil {
			return runResult{}, err
		}
	}
	if opts.pre != "" {
		// Untimed and unlogged, like the shell driver's `bash -c "$pre" >/dev/null 2>&1`.
		_ = exec.Command("bash", "-c", opts.pre).Run()
	}
	loadRaw, _ := output("sysctl", "-n", "vm.loadavg")
	load := parseLoadAverage(loadRaw)

	logFile, err := os.Create(logPath)
	if err != nil {
		return runResult{}, err
	}
	defer logFile.Close()

	cmd := exec.Command(opts.command[0], opts.command[1:]...)
	cmd.Stdout, cmd.Stderr = logFile, logFile
	cpu0 := childrenCPU()
	t0 := time.Now()
	runErr := cmd.Run()
	wall := time.Since(t0).Seconds()
	cpu := childrenCPU() - cpu0

	exitCode := 0
	var exitErr *exec.ExitError
	switch {
	case runErr == nil:
	case errors.As(runErr, &exitErr):
		exitCode = exitErr.ExitCode()
	default:
		return runResult{}, fmt.Errorf("couldn't start %s: %w", opts.command[0], runErr)
	}
	// Rounded to the hundredth here, as the report prints it, so the stats work on the same numbers the rows show.
	return runResult{load: load, wall: round2(wall), cpu: round2(cpu), exitCode: exitCode}, nil
}

// setMarker rewrites the first occurrence of anchor in path, along with any `[bench <n>]` marker already behind it, to
// anchor followed by suffix. An empty suffix restores the original text.
func setMarker(path, anchor, suffix string) error {
	src, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	re := regexp.MustCompile(regexp.QuoteMeta(anchor) + `(?: \[bench \d+\])?`)
	loc := re.FindIndex(src)
	if loc == nil {
		return fmt.Errorf("anchor not found in %s: %s", path, anchor)
	}
	var out []byte
	out = append(out, src[:loc[0]]...)
	out = append(out, anchor+suffix...)
	out = append(out, src[loc[1]:]...)
	return os.WriteFile(path, out, 0o644)
}

// childrenCPU is the user + system seconds of every child process waited for so far, the same figure the shell
// driver read from perl's `times`.
func childrenCPU() float64 {
	var ru syscall.Rusage
	if err := syscall.Getrusage(syscall.RUSAGE_CHILDREN, &ru); err != nil {
		return 0
	}
	return seconds(ru.Utime) + seconds(ru.Stime)
}

func seconds(tv syscall.Timeval) float64 {
	return float64(tv.Sec) + float64(tv.Usec)/1e6
}

func round2(v float64) float64 {
	rounded, _ := strconv.ParseFloat(strconv.FormatFloat(v, 'f', 2, 64), 64)
	return rounded
}

// parseLoadAverage turns sysctl's `{ 1.23 2.34 3.45 }` into `1.23 2.34 3.45`.
func parseLoadAverage(raw string) string {
	return strings.Join(strings.Fields(strings.NewReplacer("{", "", "}", "").Replace(raw)), " ")
}

// stats is the median / min / max line the report ends with.
func stats(values []float64) string {
	sorted := append([]float64(nil), values...)
	sort.Float64s(sorted)
	n := len(sorted)
	if n == 0 {
		return "no runs"
	}
	median := sorted[n/2]
	if n%2 == 0 {
		median = (sorted[n/2-1] + sorted[n/2]) / 2
	}
	return fmt.Sprintf("median %.2fs  min %.2fs  max %.2fs", median, sorted[0], sorted[n-1])
}

// output runs a command and returns its trimmed stdout.
func output(name string, args ...string) (string, error) {
	out, err := exec.Command(name, args...).Output()
	return strings.TrimSpace(string(out)), err
}
