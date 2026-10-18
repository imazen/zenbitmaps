# Farbfeld fixed-pixel append: native ARM confirmation

**Rejected as resource-invalid; no performance gain is accepted.** Production
source is unchanged by this record. The private candidate appends one expanded
RGBA8 pixel per call instead of four sample appends.

Frozen baseline: `a829fd164e51b08adbef8bccdb533c8f99635b2d`.
Frozen candidate: `d6df8c3d6e5b46a1e17acbd3c652b1ea12b00b2f`.
Both source trees are retained in the build archive below.

Native Neoverse-N1, eight cores, Rust 1.99.0. The measured build explicitly
cleared RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS. An earlier build inherited CPU
specialization, was retained separately, and was never measured. The retained
measured executable SHA-256 is
`de2880d08abdb649cbdf7e8ab4cbcd91d88914b1793789b83a62cb44a6e15dcc`.

Published zenbench 0.1.11; five fresh processes, 30 rounds, 1% margin, 95%
confidence, all ten groups. A 900-second supervisor deadline was declared before
timing. Process boundary: CPUs 2–3; measurement thread: CPU 2. The original
sampler also used CPU 2, as recorded in the live thread-affinity snapshot.

All per-process byte/spec preflights passed. Five replicates completed, but
resource flags invalidated confirmation: reported gate waits were
`[0, 30, 300, 300, 300]`. Repeated rounds can share an observation; these counts
are not counts of independent CPU-load events. No statistical interval from this
attempt establishes an optimization. No service or CPU-frequency setting was
changed for the attempt.

Full artifacts checked on **dev**:

`/mnt/v/output/zenbitmaps/coz-2026-10-18/native-fixed-pixel-arm-v1/`

- `ROOT_SHA256SUMS.json`: SHA-256
  `868678c912796a797befe122e1f94fdb009996d4d77bc4d90f3684bf46823d81`.
- `build/source.tar`: SHA-256
  `bc0440ad762134a86f1c3848226f73ef3b15ba2249aa76ddfbefbeee8e9bb54d`.
- Includes source/lock identities, portable build log/environment, exact
  executable, complete output, replicate records, predeclared protocol, live
  placement receipt, and 23 content-addressed correctness artifacts.
- No R2 or tower mirror has been verified.

Resource wrapper: `done rc=2 142s | peak-RSS 0.58GiB | min-avail 14435MiB |
peak-load 1.66`, with a 6 GiB cap and two build jobs.
