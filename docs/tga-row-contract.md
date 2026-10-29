# TGA declared row contract

`Encoder::begin(width, height, descriptor)` retains the canvas and returns
`OutputTiming::AT_FINISH`. It checks representable dimensions, descriptor,
configured dimensions and cancellation before changing the encoder. A refused
begin permits retry. Once admitted, another begin or one-shot encode refuses.

Each strip must match width and descriptor, and cannot exceed the declared
height. Finish rejects missing rows. The legacy begin-free row path remains
available and enforces consistent strips. Both paths copy only visible pixels
from strided input. Empty canvases retain the native format's behavior rather
than inventing a new nonempty requirement.

The accumulator reserves fallibly and checks dimensions and accumulated payload
limits before appending. This is full-raster buffering, not streaming output or
a proven whole-operation memory bound. Vector capacity and native output memory
are separate from retained payload. Stored Pulse is checked at begin, push and
finish; original cancellation reasons survive the shared error envelope.

## Qualification, 2026-10-29

Isolated ARM64 export of base `595949e0`, unchanged Cargo.lock and dependency
pins (core feb09f9f). `run-heavy --mem 3G --jobs 2 -- timeout 900 just
tga-declared-rows-check`, with external CARGO_TARGET_DIR and dev debug disabled.
Full logs retained under `~/tmp/tga-declared-rows-20261029/`.

Baseline: two of three controls fail: one-shot after begin is accepted, and an
unrepresentable canvas is accepted. Shared conformance alone passes. rc101,
45s, peak RSS 0.40GiB, minimum available 14390MiB.

Repair: three controls, 46 existing TGA conformance tests and the shared
contract test pass, plus strict library/regression Clippy. rc0, 21s, peak RSS
0.35GiB, minimum available 14421MiB. Final run adds a fourth control that checks
TimedOut at all three row boundaries; all prior tests and Clippy rerun unchanged:
rc0, 1s, peak RSS 0.16GiB, minimum available 15008MiB.

The row controls compare output to the existing one-shot path, including padded
input, invalid-strip recovery and missing/excess rows. They are protocol and
same-native byte-parity evidence, not a new independent TGA decoder oracle or
codec-performance measurement. No existing assertion was changed. No broad
consumer repin or cross-platform qualification follows from this focused run.

Source SHA256: `ec62e5b158066c5545a83e7425af400a68d25c6a8eea4b4e7d957779615b7ee2`.
Test SHA256: `09fea9da5b9eeb44d29fc7a89aae9be51df7e3cc894f345e4d5315dda65728e2`.
Lock SHA256: `c8159848c9381d5468f5a06bb8dc9d004cf263b20c8837265255bd2fd2d87c00`.
