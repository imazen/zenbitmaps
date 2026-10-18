# ff-bench — farbfeld encode Confirm benchmark (v2)

`run.py` builds one benchmark binary containing **two immutable zenbitmaps
revisions** and registers zenbench same-binary groups, so a single
`--mode=confirm` run produces paired log-ratio comparisons for `candidate`
(row-extent Rgba8 encoding, on top of landed F6) vs `baseline` (main + F6)
vs `aa` (baseline against itself — the A/A noise control). The decode
control registers `olddecode` / `newdecode` / `oldaa` so the
"decode is unchanged" claim is measured, not assumed.

## Usage

```
python3 tools/ff-bench/run.py --scratch <newdir> \
    --baseline a92accff501d22af2b75405a7b7b99c9520f2fb4 \
    --candidate <row-revision> --verify --dry-run [--offline] \
    [--bench-arg=--cpu-set=8]
```

Actions:

- `--verify` runs the `verify` binary **outside any timer**: complete
  old==new byte comparison plus an independent spec oracle on every cell
  (Rgba8 `[v,v]` expansion, Rgba16 native→BE swap, Rgb8 alpha=0xffff),
  including the `h=0` structural probe and full decoded-pixel checks for
  the decode control.
- `--dry-run` resolves the Confirm plan and prints the executable SHA256
  and CPU placement. **No timing is ever launched by this driver.**

`--bench-arg` accepts **only** whitelisted placement flags:
`--cpu-set=<ids>` and `--cpu-policy=auto|performance`. Everything else
(`--mode`, `--processes`, `--rounds`, `--margin`, `--confidence`,
`--workload-id`, `--timeout`, `unrestricted`, unknown flags) is refused
with exit 2 before any filesystem mutation. `cli-check.sh` is the negative
test battery.

## Frozen Confirm plan

```
--mode=confirm --workload-id=zenbitmaps-farbfeld-encode-v2 \
--processes=5 --rounds=30 --margin=1 --confidence=0.95
```

zenbench 0.1.11 spawns the five fresh child processes itself (re-exec of
the same binary); per-process paired log ratios combine via Student-t.
**The native summary exit is not the acceptance gate.** Acceptance (root):
every `aa`/`oldaa` interval wholly inside [-1%,+1%]; `newdecode` and the
untouched-layout groups equivalent within 1%; primary Rgba8 cells
(64/256/1024/4096/63x65/4097x3) improved beyond 1%; `ff-rgba8-1x1`
non-inferior at 1%. Gates are frozen before measurement.

## Per-process parity gate

Every fresh child process re-runs `register()`, which first calls the
shared `preflight()` module (`harness/lib.rs`, compiled into both the bench
target and `verify`): complete old==new byte compare + spec oracle on every
cell before any benchmark callback exists. A mismatch aborts the process —
and the whole confirm — loudly.

## Artifacts

With `$FF_BENCH_ARTIFACT_DIR` set (the driver sets it to
`<scratch>/artifacts`), each unique input and encoded/decoded output is
exported once as `<tag>-<sha256>.bin` — CAS dedup across the five children
via pid-unique temp + atomic `hard_link`; a pre-existing object is read
back and must byte-match. A `manifest-<sha>.json` maps each case to
dims/layout/seed/hashes/source SHAs. Export failure is fatal; assertions
never skip.

## Placement

Confirm refuses on unconstrained asymmetric topologies. dev has two L3
classes (cpus 0-7/16-23 and 8-15/24-31); runs require an enforced uniform
boundary, e.g. a cpuset scope confined to one class plus `--cpu-set` naming
it. `--cpu-policy=unrestricted` is refused by the driver — it can never
produce evidence.

## Timing scope

The whole public `encode_farbfeld` / `decode_farbfeld` call — output
allocation and destruction included — is inside `b.iter`; the decoded
`DecodeOutput` is forced through `black_box` before drop. No hashing or
correctness work is in the timed region.

## Identity

`identity.json` records: resolved commit SHAs, per-file `encode.rs` SHA256s,
all tool-file SHA256s, the tool's commit (strict: fails if tool files don't
match it byte-for-byte), rustc/cargo versions, recorded build env allowlist
(fails on `target-cpu=native`/`target-feature`), cargo config files + SHAs,
`cargo metadata` package identities, harness `Cargo.lock` SHA, resolved
`zenbench` version, the frozen `control_mapping`, binary SHA256s (exactly
one `ff` bench exe, hard-fail otherwise), and per-action results.

Every executable requires `FF_BENCH_ARTIFACT_DIR`; the preparation driver
sets it to the new scratch directory's `artifacts/` child. Validation or
dry-run failure makes the driver fail. Shared harness code is a private
module in the two executable targets, not a library API. Rgba16 oracle
conversion uses native-endian words, including on big-endian hosts.
