# SeatTrellis native bridge

In-process C ABI over the shared Rust application/domain layers. Build with
`cargo build --release --locked -p seattrellis-native-bridge` to produce
`seattrellis_bridge` as a static library, dynamic library and Rust library.

Use the canonical
[`bindings/include/seattrellis.h`](https://github.com/FrankFu916/seattrellis/blob/main/bindings/include/seattrellis.h).
The [bridge guide](https://github.com/FrankFu916/seattrellis/blob/main/docs/native-bridge.md)
documents protocol, memory ownership, session/cancellation semantics and quotas.
The [macOS preview](https://github.com/FrankFu916/seattrellis/tree/main/clients/macos)
is the first platform consumer. Mobile/other-platform clients remain separate
development and target-OS acceptance work.

```sh
cargo test --locked -p seattrellis-native-bridge
cargo build --locked -p seattrellis-native-bridge
python3 crates/seattrellis-native-bridge/tests/abi_smoke.py target/debug/libseattrellis_bridge.so
```

Use the `.dylib` path on macOS and `seattrellis_bridge.dll` on Windows. Python is
only this foreign-call test runner; the library does not embed Python or a web
server. C11 header/layout/link checks are in `tests/header_smoke.c`.
