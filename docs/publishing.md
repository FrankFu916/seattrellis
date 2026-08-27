# Publishing and Release Integrity

SeatTrellis v2.0.0 is released. This page documents the repeatable process for
subsequent v2 releases. The v2 channels are GitHub Releases for prebuilt CLI/App
binaries and desktop bundles, and crates.io for the CLI source package. The
Python 1.9.0 line remains a separate frozen legacy release on
`v1.x-maintenance`.

## Release assets

### GitHub Release

Create a tag `v<version>` on the reviewed commit and publish the release. The
Rust workflow then:

1. builds the React workbench and embeds it in the App server;
2. builds `seattrellis_cli` and `seattrellis_app` for Linux, Windows, and macOS;
3. collects six CLI/App binaries and attaches their `SHA256SUMS`;
4. runs long-run quality gates and the no-Python-runtime scan for release
   artifacts.

`v1.*` tags are handled by the maintenance line and do not receive Rust
binaries.

### Desktop bundles

The Tauri workflow builds macOS `.app`/`.dmg`, Windows MSI/NSIS, and Linux `.deb`
bundles and attaches them to the corresponding release. Desktop bundles are
**unsigned by the owner's release policy**. The workflow attaches a separate
`DESKTOP-SHA256SUMS`; verify it in addition to the general `SHA256SUMS` file.
On first launch, macOS may require **Open** from the context menu and Windows
may show a SmartScreen prompt.

### crates.io

```bash
cargo publish -p seattrellis_cli
```

`cargo install seattrellis_cli` installs the CLI from crates.io. Before
publishing, make sure the crate version and GitHub tag match, and verify a clean
installation in an isolated environment.

## Required access

- The publisher must have a verified crates.io account and permission for
  `seattrellis_cli` and its release dependencies.
- The GitHub publisher needs release permission; the desktop workflow requires
  `contents: write`.

## Release candidate policy

Every release candidate uses a unique pre-release version, such as
`2.1.0-rc.1`. After approval, restore the final version and run the full gate
again. Do not reuse a crates.io version or publish a candidate version as the
final release.

Local preflight:

```bash
seattrellis_cli --version
seattrellis_cli doctor
seattrellis_cli validate --problem problem.json
seattrellis_cli solve --problem problem.json --output plan.json
```

## Failure and rollback

- If build, archive, or installation verification fails, do not create the
  release or reuse its version. Fix the issue and publish a new candidate
  version.
- A published GitHub tag is not deleted or rewritten. Release assets may be
  replaced only before publication and only under the repository's release
  policy.
- A crates.io version cannot be overwritten. Yank an affected release when
  appropriate and publish an incremented patch version.
- Rollback means publishing a new patch release; it does not remove an existing
  tag or rewrite an already published artifact.
