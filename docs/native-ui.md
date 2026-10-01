# Native client direction

Decision date: 2026-10-01. The existing React workbench and Tauri shell remain the working product while dedicated clients are developed. The portable class-document, editor-command, repair, audit and cancellation interfaces are the shared foundation. This document selects the platform direction; it does not claim those clients already exist.

## Platform choices

| Platform | Client technology | Design authority | System integration |
| --- | --- | --- | --- |
| macOS | SwiftUI with AppKit where desktop APIs are required | [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/) | Document windows, menu commands, keyboard shortcuts, native open/save, printing, VoiceOver |
| iOS/iPadOS | SwiftUI with UIKit where required | Apple HIG | Document picker, sandbox access, adaptive navigation, Dynamic Type, VoiceOver, pointer/keyboard support on iPad |
| Windows | WinUI 3 / Windows App SDK | [Fluent and Windows design](https://learn.microsoft.com/windows/apps/design/) | Native pickers, commands, high contrast, UI Automation, keyboard and pen input, printing |
| KDE Linux | Qt 6 Quick with KDE Kirigami | [KDE HIG](https://develop.kde.org/hig/) | Desktop portals, KDE theme, accessibility, native print/file dialogs |
| GNOME Linux | GTK 4 with libadwaita | [GNOME HIG](https://developer.gnome.org/hig/) | Adaptive panes, desktop portals, AT-SPI, GNOME theme and dialogs |
| Android | Kotlin + Jetpack Compose | [Material 3](https://m3.material.io/) and [Android app quality](https://developer.android.com/quality) | Storage Access Framework, adaptive layouts, TalkBack, font scaling, system back navigation |
| Browser | React, TypeScript and semantic HTML | Browser accessibility and responsive web conventions | File System Access where supported, upload/download fallback, future local WASM Worker |

AppKit is a macOS framework and cannot be the iOS implementation. Qt is a useful portable desktop option, but a generic Qt skin does not automatically meet the Apple, Fluent or GNOME guidelines. KDE and GNOME variants share business logic, documents and acceptance tests; they need different presentation layers.

## Shared boundary

Every client sends coarse operations to the same application/domain layers: import, generate, cancel, apply an editor command, repair, audit, serialize/open a class document and export. The editor state is a display projection, not the full student roster. Full source fields and solve context live in the document; saved revisions and all locks survive reopening. Clients must not independently implement rule semantics or scoring.

Desktop prototypes can use the authenticated loopback API. Mobile clients should use an in-process bridge: a bounded, versioned C ABI or a reviewed generated bridge such as UniFFI. The existing server contract is a transport reference, not a claim that a mobile library ABI has shipped. Explicit ownership, error/status mapping, cancellation, thread affinity and maximum payload sizes must be specified before adding any bridge. Swift/WinUI/Qt clients do not require replacing the Rust solver with C++.

## Sequence and acceptance

Implement one vertical client slice first: full roster → solve → keyboard and touch seat movement → lock → save → close → reopen → repair → audit → export. Use the same synthetic 40/60/80-student documents across clients. Next add candidate comparison and rotation; only then promise full parity. Windows and macOS are the first desktop slices, followed by Android/iPad and the Linux variants as target-device testing becomes available.

A client is releasable only after testing its actual target OS: IME and CJK input, font scaling, high contrast/dark mode, screen-reader completion of the full flow, keyboard focus, touch targets, reduced motion, real file access, cancellation, system printing and offline operation. Record startup time, idle RSS and interaction latency on the same machine before claiming a speed or memory improvement. Verify native packaging, signing and update behavior separately from the solver tests.

The current Linux cloud environment cannot build or certify SwiftUI, WinUI or Android clients. No empty skeleton application is presented as a finished native product. Track each client against this acceptance flow and keep the current workbench available until a replacement reaches parity.
