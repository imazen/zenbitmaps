#!/usr/bin/env bash
# Negative CLI tests for tools/ff-bench/run.py: non-whitelisted --bench-arg
# values must exit 2 BEFORE any filesystem mutation. Run from the repo root.
set -u
cd "$(dirname "$0")/../.."
T="${TMPDIR:-$HOME/tmp}/ffbench-cli-check.$$"
fails=0

for arg in --margin=99 --mode=iterate --processes=3 --rounds=1 \
           --confidence=0.5 --workload-id=x --timeout=5 \
           --cpu-policy=unrestricted --bogus --cpu-set= --cpu-set=8a; do
    python3 tools/ff-bench/run.py --scratch "$T" --candidate x --bench-arg="$arg" \
        >/dev/null 2>&1
    rc=$?
    if [ "$rc" -ne 2 ]; then
        echo "FAIL  $arg -> rc=$rc (expected 2)"
        fails=$((fails + 1))
    else
        echo "ok    $arg refused rc=2"
    fi
done

# Whitelisted flags must pass validation (they proceed to a later failure —
# the bogus candidate 'x' cannot be resolved — not exit 2 at arg parsing).
for arg in --cpu-set=8 --cpu-set=0-7,16-23 --cpu-policy=auto --cpu-policy=performance; do
    python3 tools/ff-bench/run.py --scratch "$T" --candidate x --bench-arg="$arg" \
        >/dev/null 2>&1
    rc=$?
    if [ "$rc" -eq 2 ]; then
        echo "FAIL  $arg wrongly refused"
        fails=$((fails + 1))
    else
        echo "ok    $arg accepted (later rc=$rc)"
    fi
done

if [ -e "$T" ]; then
    echo "FAIL: scratch dir $T was created by a refused invocation"
    fails=$((fails + 1))
fi

if [ $fails -eq 0 ]; then
    echo "cli-check: PASS"
else
    echo "cli-check: $fails FAILURES"
    exit 1
fi
