# Release Checklist

**Current release: v2.0.0 (released).** Use this checklist for a subsequent
public v2 release. Replace `<version>` with the exact crate version and `<tag>`
with `v<version>`; every release candidate must use a unique version because
crates.io files cannot be replaced.

## Scope and repository state

- [ ] Every promised item is implemented, documented, and covered by an
      acceptance test, or explicitly moved to a later milestone.
- [ ] The pull request is reviewable as focused commits with no unresolved
      review threads.
- [ ] `CHANGELOG.md` describes user-visible additions, changes, and fixes.
- [ ] Schema changes include migration behavior, committed schemas, and fixtures.
- [ ] The release checkout is clean according to `git status --short`.
- [ ] Repository hygiene passes; Python is only a development/test tool and not
      a v2 runtime dependency.

## Release quality gate

Run this gate after the version scope is frozen:

- [ ] `cargo test --locked -p seattrellis_core`
- [ ] `cargo test --locked -p seattrellis`
- [ ] `cargo test --locked -p seattrellis_web`
- [ ] `cargo clippy --all-targets -p seattrellis_core -p seattrellis -- -D warnings`
- [ ] `cargo clippy --all-targets -p seattrellis_web -- -D warnings`
- [ ] `cargo build --locked -p seattrellis_desktop` with the pinned 1.88 toolchain
- [ ] `cd clients/web && npm test && npm run typecheck && npm run build`
- [ ] `cargo test --locked --workspace` against the frozen fixtures in
      `fixtures/README.md`.
- [ ] Run the current Rust candidate, rotation, and solver performance gates.
      The former Python oracle/differential gate was removed after v2.0.0 and
      must not be reintroduced.
- [ ] Run browser E2E acceptance from a clean build, including the
      `web-e2e-rust` job where Python is only the test runner.
- [ ] Launch `seattrellis_web` on `127.0.0.1` and complete import -> solve ->
      adjust -> export in the React workbench.
- [ ] Confirm GitHub Actions passes on Linux, Windows, and macOS.

## Functional acceptance

- [ ] `doctor`, `validate`, `solve`, and an export complete on a fictional
      sample problem.
- [ ] Generate several candidates and inspect recommendation, score differences,
      hard-constraint summary, and history explanation.
- [ ] Exercise manual swap, move, unseat, lock, and constrained repair; confirm
      lock state and provenance are saved.
- [ ] Exercise the project lifecycle: `project-init`, `project-list`,
      `project-info`, `project-validate`, `project-solve`, `project-export`,
      `project-rotate`, `project-edit`, `project-repair`, `project-privacy`,
      `project-pack`, and `project-restore`.
- [ ] Export teacher and public outputs in Chinese and English, including A4
      portrait and landscape where supported.
- [ ] Confirm public output contains no scores, notes, special needs, height,
      vision data, student IDs, or un-anonymized labels.
- [ ] Export and validate committed JSON Schemas. Exercise schema migration in
      dry-run and write modes, including backup creation.
- [ ] Verify all seven statuses and exit codes `0/2/3/4/5/70/130`. A timeout or
      unknown result must never be described as proven infeasibility.

## Packaging and privacy

- [ ] Build from a clean checkout so ignored or untracked files cannot enter the
      release.
- [ ] Scan the production tree and release CLI/App binaries for Python runtime
      symbols and dependencies.
- [ ] Confirm examples contain fictional data only and no real names, IDs,
      school details, notes, snapshots, exports, keys, or environment files.
- [ ] Confirm generated output and private-data directories remain ignored.
- [ ] Confirm all crate versions equal `<version>` and match `<tag>`.

## Release candidate

- [ ] Set crate versions to a unique candidate version and merge the reviewed
      candidate commit.
- [ ] Publish the candidate only when needed and verify clean installation,
      `--version`, `doctor`, and one validate/solve/export workflow.

## Public release

- [ ] Restore crate versions to `<version>` and rerun the release quality gate.
- [ ] Create a draft GitHub Release on the reviewed commit with tag `<tag>` and
      title `SeatTrellis <tag>`.
- [ ] Write release notes covering upgrade, compatibility, privacy, unsigned
      desktop bundles, and known limitations.
- [ ] Publish the release and confirm the six platform CLI/App binaries,
      `SHA256SUMS`, and desktop assets where applicable.
- [ ] Inspect unsigned `.app`/`.dmg`, MSI/NSIS, and `.deb` bundles and attach
      `DESKTOP-SHA256SUMS`.
- [ ] Publish the CLI to crates.io and verify a clean installation.
- [ ] Confirm the v1 line remains frozen at 1.9.0 on `v1.x-maintenance` and does
      not receive Rust binaries.

If publication fails after a version reaches crates.io, follow
[Publishing](publishing.md): do not replace files or rewrite the tag. Yank the
affected release when appropriate and publish a new patch version.
