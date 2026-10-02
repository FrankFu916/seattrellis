# SeatTrellis macOS native preview

This is a real SwiftUI + AppKit application for **macOS 13 or later**. It calls
the shared Rust application layer in process through ABI 1. It does not start
an HTTP server, listen on a local port, embed a web view, or require Node/Python
at runtime. Swift tools 5.9 or later are required; the package uses Swift 5
language mode and is compatible with Swift 6 toolchains. There are no external
Swift package dependencies.

## Build and test

On macOS with Xcode Command Line Tools, Rust 1.88 and the repository checkout:

```sh
cargo build --release --locked -p seattrellis-native-bridge
swift test --package-path clients/macos
bash clients/macos/scripts/build-app.sh
open clients/macos/build/SeatTrellis.app
```

The C shim includes the canonical `bindings/include/seattrellis.h`, and the
package links `target/release/libseattrellis_bridge.a`. For a custom Cargo
target directory, set `SEATTRELLIS_NATIVE_LIB_DIR` to the directory containing
that static library before running either Swift command or the build script.
The script creates **an unsigned, unnotarized preview**, not an installer or a
production release. Use an Apple Silicon host to build arm64, or an Intel host
to build x86_64; a universal distribution and signing are later work.

## Implemented flow

1. Launch with an explicitly fictional demonstration roster, import a JSON
   roster with the native open panel, or open a portable class document.
2. Inspect all student names, IDs, heights, vision and scores in a native
   table. **Edit JSON** edits the complete student objects. Unknown fields,
   attributes, notes and special needs remain in the persisted source.
3. **Generation Request** edits the full request, including room, hard/soft
   rules, history, seed and time budget. The initial request uses seed 42 and
   a two-second budget. Changing source invalidates the previous assignment.
4. **Generate** calls the Rust solver on a background queue and presents its
   exact seven-state result. Timeout/Unknown are never called a proof of
   infeasibility. Cancel has its own thread-safe path and retries the narrow
   native-control publication window without affecting a later operation.
5. Select seats in the chart, or use the accessible assignment list and the
   inspector. Move, swap, unseat, lock/unlock students or seats, undo and redo
   use the shared revisioned editor protocol. A failed edit leaves the last
   acknowledged state visible. Manual edits can violate hard constraints.
6. **Save Class** writes the actual portable class document: full class source,
   complete solve context, current assignment and locks. Close and reopen it
   to restore the editable plan. Undo history is session-local and is not part
   of the portable document.
7. **Repair Plan** fills/rearranges the current assignment while preserving
   locks, as one undoable operation. **Audit Plan** displays actual independent
   hard-constraint diagnostics and the score report for the current revision.
8. **Export** writes real SVG or HTML through the shared renderer and a native
   save panel. The public template hides scores, notes, needs, height and
   vision; optional anonymization also removes student names. JSON class saving
   is separate from public export and retains sensitive teacher input.

The preview deliberately uses one window with one native session. Menu
shortcuts include Command-O, Command-S, Shift-Command-S, Command-I,
Command-Z, Shift-Command-Z and Command-Return. Dirty documents require an
explicit save/discard choice before replacement, window close or application
quit. File reads and atomic writes honor security-scoped URLs and run off the
main thread. Input requests are capped at 8 MiB, protocol responses at 32 MiB;
portable documents have a slightly smaller limit to leave room for the request
envelope. Save failures retain the dirty state and current assignment.

## Preview limits and validation

- Student import currently accepts a UTF-8 JSON array of complete student
  objects, or an object with a `students` array. CSV/XLSX import is available
  through existing products and is not claimed by this client.
- This preview opens **single-plan** documents. It rejects candidate sets,
  rotation plans, and unsolved foreign documents without a native generation
  request before replacing the current document. Generated single-plan web
  documents can recover their request from the stored solve context; their
  full class source is retained. It never substitutes demo students for a
  foreign class.
- SVG/HTML export is implemented. Native print, PDF/Office UI, localized UI,
  multiwindow editing, sandbox entitlements, signed updates and notarization
  still require development. The current unsigned app does not request the
  App Sandbox; security-scoped access is handled for supplied URLs.
- Native model tests call the real Rust ABI for generation, swap, undo/redo,
  locks, save/reopen, repair, audit and anonymized SVG export. Deterministic
  concurrency regressions cover cancellation before native control publication,
  overlapping calls, late cancelled results, suspended imports and failed saves.
- The Linux cloud workspace cannot run SwiftUI/AppKit. The macOS CI job builds
  and tests the actual Swift package and assembles the app. Window interaction,
  VoiceOver, keyboard focus, CJK input, native dialogs and real-device usability
  still need manual macOS acceptance. An SDK build is not a claim that those
  manual checks have passed.
