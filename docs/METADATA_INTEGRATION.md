# Bitmap metadata inventory integration

The default-off `inventory` feature enables zencodec integration and the existing
[PR23](https://github.com/imazen/zenbitmaps/pull/23) walkers at `4c383dbf`.
PNM and farbfeld are included; BMP, QOI, TGA and HDR walkers follow their format
features. Normal decoding and metadata access do not require inventory or
zenmeta. The disabled trait operation returns typed unsupported.

The three original BMP crash fixtures are retained unchanged, with
[provenance](../fuzz/INVENTORY_FIXTURES.md). The parser's strict-size overflow and
palette/mask overlap repairs are retained alongside their original regressions.
Inventory invariant and part-cap failures now retain the shared typed cause.

## Validation

Native and i686 suites pass for all six walkers, the existing corpus and review
cases, original crash replay, and feature/cancellation boundary tests. Strict
clippy and no_std inventory compilation pass. The standalone fuzz target
compiles and public API snapshots are regenerated.

The independent ImageMagick pixel-extent check agreed on 15 farbfeld, 40 PNM and
67 BMP inputs; its original pinned rejection list is retained. ExifTool header
and profile checks passed, with its seven original unreadable PNM cases retained.
These are the original oracle acceptance rules, not a claim every input is
accepted by every reader. Run `just inventory-oracle` explicitly.

## Published API

The forced-patch rustdoc comparison against crates.io 0.1.5 reports 222 passing,
one failing and 31 skipped checks. The failure is the added payload in
`BitmapError::UnsupportedVariant`, already on main `e5035995`. Manual inspection
also finds 39 codec-trait associated errors migrated from `BitmapError` to
`At<CodecError>` before this extraction; the tool does not detect these across
dependency revisions. The local 0.2.0 version predates this integration.
Inventory is absent from 0.1.5 and adds no further published signature break.

## Remaining limits

The imported job-limit test deliberately expects a completed inventory annotated
with the decoder's rejection, including when input exceeds the configured cap.
That behavior is preserved pending an explicitly approved contract change; it
has not been made equivalent to the other codecs' early typed InputSize errors.
Cancellation is checked before and after the walk, not throughout every internal
byte scan. Tighter latency and allocation accounting remain review work.
