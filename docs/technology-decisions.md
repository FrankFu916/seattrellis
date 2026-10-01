# Technology decisions

Review date: 2026-10-01. A technology is replaceable; replacing it is justified by a measured product benefit, a platform requirement or an unsupported dependency. Correctness defects in data lifecycles are addressed directly, because another language or UI framework can reproduce them.

## Core and solver

Rust is not mathematically necessary for seating optimization. A C++ core can implement the same algorithms and can integrate OR-Tools directly; C can expose a compact ABI but needs more manual ownership and higher-level solver tooling. Neither is inherently faster than optimized Rust for the same algorithm. Retain Rust as the shared implementation: it already owns validation, editing, history, privacy and exports, its solver has only serde dependencies, and moving all those behaviors would create a new equivalence burden. Native UI bridges can use C ABI without rewriting the implementation.

SAT, CP-SAT and OR-Tools are separate decisions from the implementation language. OR-Tools CP-SAT can encode one student per seat, fixed assignments and forbidden pairings with Boolean/integer variables, plus integer-scaled objective terms. A plain SAT backend needs cardinality encodings and MaxSAT/Pseudo-Boolean optimization to express weighted objectives. Graph distances and history must be compiled with the same semantics as the current core. Continuous quantities need explicit integer scaling and overflow bounds in CP-SAT; a satisfiable assignment is not evidence that the weighted optimum was reached.

The [reproducible comparison](https://github.com/FrankFu916/seattrellis/tree/main/benchmarks/solver-comparison) freezes nine 40/60/80-person requests, one worker, seed 42 and two-second budgets, with three trials per case. CP-SAT was faster in all nine hard-feasibility cases, including warm model construction and adapter work. Both engines solved every feasible case and proved the constructed infeasible cases. All 36 returned assignments passed both an independent Python validator and the Rust CLI audit; both validators also rejected a deliberately invalid assignment.

| 80-person case | Rust core median | CP-SAT warm model, solve and adapter median |
| --- | ---: | ---: |
| Easy | 854.7 ms | 100.3 ms |
| Mixed hard constraints | 665.3 ms | 306.1 ms |
| Proven infeasible | 296.3 ms | 103.6 ms |

This is useful evidence for developing an optional CP-SAT backend. It compares algorithms and adapters, rather than proving a language speed ranking: the Python adapter calls OR-Tools' native C++ engine. A separate cold 60-person process took 499.1 ms, including 314.7 ms to import OR-Tools; it is one sample and excluded from the warm medians. All ten soft objectives were disabled, and weighted quality, cancellation, candidate diversity, rotation and native packaging remain untested for CP-SAT. A production backend must preserve these semantics, the full seven-status contract and a valid incumbent on timeout. Retain the Rust independent validator and shared application layers while completing those comparisons.

## Stack review

| Current technology | Alternatives worth considering | Decision and replacement trigger |
| --- | --- | --- |
| Rust / Cargo / serde | C++ with CMake, C ABI; generated bridges | Keep shared Rust. Add platform bridges when clients need them; rewrite only after measured constraints justify migration. |
| Custom CSP search and local search | OR-Tools CP-SAT, SAT/MaxSAT, SCIP | CP-SAT's measured hard-feasibility advantage justifies an optional backend; require soft-objective, lifecycle and distribution parity before production adoption. |
| React 19 / TypeScript / Vite | Vue, Svelte, Solid; native clients | Keep the web UI. Refactor state by class source, editor revisions and async operations; framework replacement alone does not resolve lifecycle defects. |
| Handwritten workbench controls | Accessible headless primitives, React Aria; design-system components | Adopt selectively for dialogs, menus and focus management; preserve dense classroom workflows. Native apps follow their own official guidelines. |
| Tauri 2 / system WebView | Platform-native clients, Qt, Electron | Retain the shipping fallback while native slices are validated. Electron adds a bundled browser and distribution cost; it offers no demonstrated benefit here. |
| Axum / Tokio loopback | Direct FFI, UniFFI, WASM Worker; other HTTP frameworks | Retain desktop/browser transport with bounded bodies, cancellation and store capacity. Prefer in-process mobile bridges and browser-local Worker for future channels. |
| JSON DTO / schemars / OpenAPI | Protobuf, FlatBuffers, CBOR | Keep inspectable, versioned JSON. Consider binary transport only after profiling payload size/latency; do not conflate public protocol versions with product versions. |
| Journaled files / fs2 locking / SHA-256 ZIP bundles | SQLite transactions, platform document APIs | Keep portable files for the present scale. SQLite is a candidate for searchable durable history; add migrations and atomic export before switching. |
| Custom CSV/Excel import | csv crate, calamine | Keep tested compatibility for now; evaluate maintained parsers against malformed input, large files, formulas and current import contracts before replacing. |
| Custom SVG/HTML and Office XML exports | Mature OOXML libraries, native print APIs | Preserve format behavior; shared XML filtering and real reader gates now protect output. Prefer a supported library when it reduces maintenance without losing layout/privacy controls. |
| fontdue / ttf-parser / system fonts | skrifa, HarfBuzz/rustybuzz shaping, vector PDF libraries | Revisit font maintenance and complex-script shaping. Searchable/accessible PDF requires actual text/shaping, not merely increasing raster resolution. |
| Raster PDF | Vector/text PDF with tagged accessibility | Planned product improvement; require embedded-font licensing, text extraction, screen-reader and target-reader validation. |
| Docusaurus / React / MDX | Astro/Starlight, MkDocs, VitePress | Keep bilingual static docs; dependencies patched, Node minimum aligned. Replace only if build cost/maintenance measurements warrant a content migration. |
| Vitest / Testing Library / Playwright | Browser-native component tests, platform UI tests | Keep web regression tools. Add native accessibility and document-reader checks alongside them; snapshot equivalence is insufficient. |
| GitHub Actions / cargo-deny / npm audit | Other CI platforms, additional scanners | Keep exact-ref shared gates for all uploads, compiler pin and version preflight. Security exceptions must describe actual supported platforms. |
| Rust MSRV 1.88 / Node 24 builds / separate TS versions | Newer supported compilers | Keep tested minimum and pinned build compiler. Do not update all dependencies blindly; web TS7 and docs TS6 have different compatibility constraints. |
| Static docs and local-only app | Hosted backend, cloud sync | Local-first stays the default. Browser-local WASM is the preferred web product experiment; hosted computation changes the student-data trust boundary. |

## Scope of refactoring

The needed web refactor is architectural and incremental: retain full class source independently of editor projections; keep saved baselines per draft; invalidate stale requests on every source edit; centralize bounded transport and cancellation; share typed restore/repair/export use cases. A later split of the large App component into class-file, generation and draft hooks should preserve those invariants and the full-flow tests. Framework rewrites and empty native shells provide no evidence of improved behavior.

No new solver runtime, mandatory account or untested UI framework is added to production by this assessment. Versions remain 2.1.0 in source until a release is deliberately prepared; the new save/open and repair capabilities are suitable for a 2.2 feature release after validation. Breaking file/API/CLI compatibility, rather than the implementation language, determines whether a major version is needed.
