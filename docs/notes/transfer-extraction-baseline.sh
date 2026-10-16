#!/usr/bin/env bash
# Times one build scenario for `docs/notes/transfer-extraction-baseline.md`, the same way before and after.
#
# Usage:
#   transfer-extraction-baseline.sh [-n RUNS] [--edit FILE ANCHOR] [--pre 'SHELL CMD'] [--no-warmup] -- COMMAND...
#
#   -n RUNS        timed runs (default 5)
#   --edit F A     before every run (warm-up included), rewrite the string literal fragment A in file F (repo-relative)
#                  to `A [bench <i>]`, so incremental compilation has to redo real work (never a `touch`). The file is
#                  restored to A on exit, even on Ctrl-C.
#   --pre CMD      untimed shell command before every run (for example `cargo clean -p cmdr`)
#   --no-warmup    skip the untimed warm-up run (use it when every run starts from a clean state anyway)
#
# Prints, per run: the 1/5/15-minute load average just before it, wall seconds, and CPU seconds (user + sys of all
# children). CPU seconds survive a loaded machine far better than wall time (see `archive-extraction-baseline.md`).
# Ends with median / min / max of both. A run whose output mentions cargo's "Blocking waiting for file lock" is flagged
# LOCK-WAIT: it timed another session, not this build, so drop it and re-run.
#
# Runs from the repo root regardless of the caller's directory.

set -euo pipefail

runs=5
edit_file=""
edit_anchor=""
pre=""
warmup=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    -n) runs="$2"; shift 2 ;;
    --edit) edit_file="$2"; edit_anchor="$3"; shift 3 ;;
    --pre) pre="$2"; shift 2 ;;
    --no-warmup) warmup=0; shift ;;
    --) shift; break ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ $# -gt 0 ]] || { echo "no command given (put it after --)" >&2; exit 2; }

repo_root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
cd "$repo_root"

set_marker() { # $1: marker suffix ("" restores the original)
  ANCHOR="$edit_anchor" SUFFIX="$1" perl -0pi -e 's/\Q$ENV{ANCHOR}\E(?: \[bench \d+\])?/$ENV{ANCHOR}$ENV{SUFFIX}/' "$edit_file"
}

if [[ -n "$edit_file" ]]; then
  grep -qF -- "$edit_anchor" "$edit_file" || { echo "anchor not found in $edit_file: $edit_anchor" >&2; exit 2; }
  trap 'set_marker ""' EXIT
fi

log="$(mktemp -t transfer-baseline)"

# Edits, runs the untimed pre-step, then runs the command once. Prints "<load1> <load5> <load15> <wall> <cpu> <exit>".
one_run() { # $1: run index, then the command
  local idx="$1"
  shift
  if [[ -n "$edit_file" ]]; then set_marker " [bench $idx]"; fi
  if [[ -n "$pre" ]]; then bash -c "$pre" >/dev/null 2>&1; fi
  local load
  load="$(sysctl -n vm.loadavg | tr -d '{}' | xargs)"
  # The command's own output goes to $log; perl's summary line goes to stdout.
  local result
  result="$(perl -MTime::HiRes=time -e '
    open(STDOUT_SAVE, ">&", \*STDOUT) or die;
    open(STDOUT, ">>", $ENV{LOG}) or die; open(STDERR, ">>", $ENV{LOG}) or die;
    my $t0 = time; my @c0 = times;
    my $rc = system(@ARGV);
    my $t1 = time; my @c1 = times;
    printf STDOUT_SAVE "%.2f %.2f %d\n", $t1 - $t0, ($c1[2] - $c0[2]) + ($c1[3] - $c0[3]), $rc >> 8;
  ' -- "$@")"
  echo "$load $result"
}

export LOG="$log"
echo "# $(date '+%Y-%m-%d %H:%M:%S')  commit $(git rev-parse --short HEAD)  $(rustc -V)"
echo "# command: $*"
[[ -n "$edit_file" ]] && echo "# edit: $edit_file  \"$edit_anchor\""
[[ -n "$pre" ]] && echo "# pre: $pre"

if [[ "$warmup" == 1 ]]; then
  : >"$log"
  read -r _ _ _ w_wall _ w_rc < <(one_run 0 "$@")
  echo "warm-up  wall ${w_wall}s  exit $w_rc"
  [[ "$w_rc" == 0 ]] || { echo "warm-up failed; output in $log" >&2; exit 1; }
fi

walls=()
cpus=()
for ((i = 1; i <= runs; i++)); do
  : >"$log"
  read -r l1 l5 l15 wall cpu rc < <(one_run "$i" "$@")
  flag=""
  grep -q "Blocking waiting for file lock" "$log" && flag="  LOCK-WAIT"
  [[ "$rc" == 0 ]] || flag="$flag  EXIT $rc (output in $log)"
  printf 'run %d  load %s %s %s  wall %7.2fs  cpu %8.2fs%s\n' "$i" "$l1" "$l5" "$l15" "$wall" "$cpu" "$flag"
  walls+=("$wall")
  cpus+=("$cpu")
done

stats() { # median / min / max of the arguments
  printf '%s\n' "$@" | sort -n | awk '{ v[NR] = $1 } END {
    m = (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2
    printf "median %.2fs  min %.2fs  max %.2fs", m, v[1], v[NR]
  }'
}
echo "wall: $(stats "${walls[@]}")"
echo "cpu:  $(stats "${cpus[@]}")"
rm -f "$log"
