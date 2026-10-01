# Audit remediation, 2026-10-01

The audit started from main commit `c376d60` (product 2.1.0). This change repairs the confirmed behavior defects and implements the missing save/open and GUI repair flows. [Technology decisions](technology-decisions.md) and [native client direction](native-ui.md) describe the implementation choices and remaining platform work. A framework selection is not a completed client.

## Implemented repairs

| Finding | Result | Regression evidence |
| --- | --- | --- |
| A01 Full roster lost after generation | Full class source stays independent of editor projections, including height, score, vision, needs, notes and attributes. | Two-generation frontend lifecycle test |
| A02 Rules silently ignored | HTTP, application and project boundaries use strict parsing; malformed hard/soft shapes and unknown rules are rejected. | Core input-boundary, IO project and HTTP tests |
| A03 v2 migration dry-run writes | All versions validate before dry-run; preview never writes. | CLI lifecycle subprocess tests and shared artifact validation |
| A04 Recovery rolls back live writers | Cross-process root locks cover recovery, stage and commit. | Real-process active/crashed transaction tests |
| A05 Locks/source lost after save/open | Saved documents preserve complete solve contexts and locks; restored drafts can edit, repair, audit and export. | CLI roundtrip, portable document and rotation tests |
| A06 Unsaved edits disappear on candidate switch | Saved baselines track class source and each draft separately; selecting a candidate cannot mark it saved. | GUI dirty-state and candidate-switch tests |
| A07 Migrated projects cannot reopen | v2 envelope and legacy inputs share compatible workspace readers; unknown/future v2 fields remain strict. | IO migration-consumer roundtrip tests |
| A08 Output directory ignored / partial writes | Configured outputs are honored; result and report are one transaction. | CLI output-directory and failed-report tests |
| A09 Coordinate collapse / layout panic | Row/column fallback and canonical seat domains are shared and validated. | IO coordinate and core input-boundary regressions |
| A10 Topology and diversity scores wrong | Scoring uses solve topology; diversity ratio converts to 0–100, with unavailable single-candidate dimension. | Expected-value semantic tests |
| A11 Repair omits hard-rule relations | Affected closure includes distance and group rules; locks remain fixed. | Repair regressions for distance/group cases |
| A12 Misleading statuses / unstable input focus | Timeout, unknown, cancellation and infeasibility have distinct messages; row keys remain stable. | Four-status UI and continuous-ID-input tests |
| A13 Unbounded request/shutdown waits | Authentication precedes body reads; body/request/shutdown deadlines and cooperative job cancellation are enforced. | HTTP boundary, cancellation and lifecycle tests |
| A14 Invalid XML / hidden ID leakage | Shared XML1.0 filtering with non-sensitive warnings; ID-only students get localized neutral labels when IDs are hidden. | Independent XML parsing and privacy/export regressions |
| A15 Ungated release uploads | CLI/server and desktop require shared exact-tag gates; all product versions must match the tag before upload. | Actionlint and version-preflight regressions; remote CI remains the execution authority |

Additional repairs cover finite arithmetic, bounded numerical inputs, wrapping rotation seeds, lazy graph-distance preparation, deadline/cancellation during preparation, indexed period-based history lookback, shared pack/restore size limits, bundle reserved names, cycle-safe contained project listing, atomic unique restore/rotation names, CLI history forwarding, export settings persistence, localized XLSX headings, accessible HTML assignment tables and real Word pagination. Partial soft-rule JSON now uses declared model defaults rather than accidental zero values.

Saved rotation periods now use their captured source before consulting current project inputs, so a changed roster or invalid replacement rule file cannot break a complete saved plan. Embedded web assets are copied to immutable build output, avoiding stale filenames after a later frontend build. Exclusive file creation requires hard-link support: unsupported filesystems or permissions produce an actionable error and leave the target intact, rather than using a racy overwrite fallback. Test this capability before deploying projects on FAT/exFAT or restricted mounts.

The input contract rejects heights outside 0–300 cm and scores outside ±1e9. Limits are 1,000 students, 10,000 seats, 100,000 edges/hard constraints; graph-distance operations additionally limit seats to 2,000 to bound the matrix. These deliberate validation changes reject unrealistic or resource-exhausting input. Legacy optional result cost remains readable; non-finite supplied cost is rejected.

Both npm dependency trees are patched and audited without advisory exceptions. The obsolete Python benchmark and PyInstaller spec are removed. The active benchmark has a distinct frozen corpus, explicit metadata/hashes and a subprocess deadline; it no longer claims to use the Rust long-run generator or to remove timing noise.

## Verification record

Local integrated validation passed 802 Rust tests (8 opt-in tests excluded from the routine run), 272 frontend tests and all 8 real Chromium workflows. Strict workspace Clippy passed. The ignored release candidate, long-run and rotation gates were run separately, including 500 repeated solves, the planted-feasible corpus, cancellation and independently validated rotation periods. Six fuzz targets each completed 3,000 bounded iterations without a reported failure; this is bounded evidence, not exhaustive coverage. The Tauri crate was excluded from local workspace compilation because its platform libraries are unavailable here.

Generated contract checks, 12 tooling tests for release/security checks and Actionlint passed. Both npm trees have zero reported vulnerabilities, and cargo-deny passed the repository policy, including its documented Rust advisory exceptions. The dependency check used Git CLI transport for the advisory database because the cloud proxy rejected the default fetch; the policy was not relaxed. Frontend and bilingual documentation type checks/builds passed. Remote OS-matrix and release CI results must still be checked after pushing.

The CLI performance gate passed unchanged relative and absolute thresholds: 40/50/60/80-person medians were 142.93/286.97/660.90/2207.31 ms on this host. Its revised input explicitly preserves the historical effective hard-only workload after the soft-default fix; no timing baseline was rewritten. Cross-machine comparisons remain limited by missing historical metadata.

The isolated [solver comparison](https://github.com/FrankFu916/seattrellis/tree/main/benchmarks/solver-comparison) contains nine cases and 27 trials. All 36 feasible assignments passed two validators; constructed infeasibility also has an independent graph proof. CP-SAT was faster on this hard-only corpus, justifying further optional-backend development. It does not validate weighted quality, mobile distribution or a replacement C++ implementation; see [technology decisions](technology-decisions.md) for timings and limits.

The local independent reader verification covers seven 40/60-student configurations, portrait/landscape, identifiers on/off and 25 mm margins; all fit one page and retain every student name. This uses LibreOfficeDev 26.8 plus Poppler; Microsoft Word and WPS still need target-reader verification.

## Further development

The selected native clients, full browser-local WASM/Worker edition, complex-script shaping, searchable/tagged PDF, native signing and target-OS accessibility certification remain development work. The existing workbench is the supported fallback. The current Linux cloud can validate the Rust server, CLI, web workbench and documents; it cannot certify Apple, WinUI or Android builds, or the Tauri window without its platform libraries. Source versions stay at 2.1.0 until a deliberate release update; the new workflow capabilities are candidates for 2.2.
