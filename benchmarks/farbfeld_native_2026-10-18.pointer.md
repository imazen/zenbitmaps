# Native farbfeld confirmation, 2026-10-18

**Invalid; no speedup accepted and no encoder optimization landed.** All five
fresh processes completed 30 rounds per group, but 66 rounds were flagged by
resource admission. Complete byte/spec parity passed before timing in every
process. No samples were removed and no thresholds were changed.

Baseline: `aa017f70f03d3a9ebb9dc00eb9d707e61c3b8c6a`.
Candidate: `713901cd6d40289e90df500e1fa264d3d9c8b565` (unpublished).
The [source patch](farbfeld-row-candidate_2026-10-18.patch) retains the tested
candidate independently of that unpublished commit.

Binary SHA256: `b365c07c7a030619e3288fb0b26abb74b60cec1b052869786ea897a239237f0d`.
Native zenbench 0.1.11: Confirm, five fresh processes, 30 fixed rounds,
1% practical margin, 95% family confidence, workload
`zenbitmaps-farbfeld-encode-v2`. The frozen acceptance map is in
`acceptance.json`; native exit zero alone would not establish acceptance.

Measured on r3500, CPU 2 enforced by a cpuset scope, stock powersave governor
and balance_performance EPP. The successful dry-run records exact placement.
Earlier placement-only dry-run failures are retained too.

```
./ff --mode=confirm --workload-id=zenbitmaps-farbfeld-encode-v2 \
  --processes=5 --rounds=30 --margin=1 --confidence=0.95 --cpu-set=2
```

The guarded invocation and full logs are retained with the data:
`run-heavy: done rc=2 105s | peak-RSS 0.58GiB | min-avail 13887MiB | peak-load 0.76`.

The flagged rounds per process were 30, 5, 1, 30, 0. For process zero,
all 30 rounds in `ff-rgba8-63x65` used the same cached observation:
CPU load 0.09285339037577314, heavy-process count 0, available RAM
14837444608 bytes, temperature 44.625 C. This does **not** establish whether
the load was foreign or misattributed. The next experiment diagnoses raw
owned/global CPU counters and checks an independently launched foreign-load
control before any new performance acceptance attempt.

## Retained data

Local canonical path, checked on dev on 2026-10-18:
`/mnt/v/output/zenbitmaps/coz-2026-10-18/native-confirm-v1-r3500/`.
It contains full raw child/aggregate data, all logs, 23 content-addressed
input/output artifacts, source/build/dependency identities, the exact binary,
frozen acceptance policy and root resource audit. The original remains on
r3500 at `~/tmp/zen-coz-bitmaps-2026-10-18/native-confirm-v1/`.
R2 and tower mirrors: not created.

Manifest SHA256 (`ROOT_SHA256SUMS.json`): `b451f4f633f080033140f190202672ea74698fb7caa672937374f1695a1328fe`.
Resource audit SHA256: `7bb5f8a0e696aa9dcf29156747b4ef6939dfe00e1313180bb28ae58d74b55805`.
Candidate patch SHA256: `37a3c9c2591153b2e9f440fcbfb0d758ac6580e53d3e5722759a565e05e97648`.
