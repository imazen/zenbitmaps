# Farbfeld zero-height correction — 2026-10-18

Source fix: `325bd95192bf24f94919931d039b8262311f1025`.
No performance claim. Supported layouts keep the existing 64-bit header-only
result at zero height on i686 as well. Width-zero validation, initial
cancellation and unsupported layouts retain their prior error behavior.

Independent review used immutable source archives. On i686, the original
source panicked in 23 of 35 checked-build cases and 13 of 35 ordinary-release
cases; the fix panicked in none. On x86-64, both sources returned identical
results. An independent all-layout probe found no error-kind, validation-order
or cancellation-count change. Error source lines below the insertion move by
20 lines. No format-validity claim is made for zero-height images.

The merged fix-plus-row-candidate archive passed the full all-features suite
(506 passed, 12 preexisting ignored) and clippy. That is correctness evidence,
not approval or performance acceptance of the separate row candidate.

Final in-repo coverage additionally pins all three unsupported layouts at
seven widths and the exact initial cancellation variant. Run
`just check-farbfeld-zero-height` for x86-64, native i686 debug/release,
formatting and clippy. On dev:
`run-heavy: done rc=0 0s | peak-RSS 0.05GiB | min-avail 37066MiB | peak-load 10.92`.

Retained artifacts checked on dev:

- `/mnt/v/output/zenbitmaps/coz-2026-10-18/f6/`: source-identified old/fixed
  probe outputs.
- `/home/lilith/tmp/f6-review-2026-10-18/`: independent review script, complete
  log, source archives, all-layout probe, checked and unchecked results.
- `/home/lilith/tmp/handoff/zen-coz-bitmaps/root-f6-final-gates.log`: final
  regression checks including unsupported layouts.

The independent review's resource record:
`run-heavy: done rc=0 53s | peak-RSS 0.38GiB | min-avail 36294MiB | peak-load 14.30`.

The optional apidoc snapshot check has a separately reported preexisting
feature snapshot mismatch; no snapshot or expectation was regenerated here.
No R2 or tower mirror was made for these diagnostic artifacts.
