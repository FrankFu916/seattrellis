# In-process native bridge

The `seattrellis-native-bridge` workspace crate exposes the shared application
layer to native clients through a C ABI. It builds `seattrellis_bridge` as a
static library, dynamic library and Rust library. The macOS SwiftUI preview is
its first consumer. This is an early integration API; iOS, Windows, Android,
Qt and GTK clients still require their own builds and target-platform tests.

```sh
cargo build --release --locked -p seattrellis-native-bridge
```

The canonical header is
[`bindings/include/seattrellis.h`](https://github.com/FrankFu916/seattrellis/blob/main/bindings/include/seattrellis.h).
Use `SEATTRELLIS_STATIC` for static linking on Windows; dynamic imports use
`cdecl`. `size_t` and the buffer layout follow the target architecture.

## Sessions, threads and ownership

Each nonzero opaque session ID owns independent editor drafts and full solve
sources. IDs are not reused. Dispatch is synchronous and belongs on a client
background thread. Overlapping calls on one session return `session_busy`;
separate sessions can operate concurrently. Cancel and destroy do not wait for
application/store locks. Each dispatch receives a fresh cancellation control.
Clients must also cancel work queued before dispatch, since cancelling an idle
session does not cancel its next call.

| Function | Contract |
| --- | --- |
| `seattrellis_abi_version()` | Returns ABI major version 1. |
| `seattrellis_session_create()` | Creates a session; returns zero if allocation/quota fails. |
| `seattrellis_session_dispatch(id, input, len)` | Returns owned UTF-8 JSON bytes without a trailing NUL. |
| `seattrellis_session_cancel(id)` | Returns 1 for active cancellation requested, 0 for idle, −1 for an invalid ID. |
| `seattrellis_session_destroy(id)` | Removes the session and cancels an active call; that call retains its own lifetime until return. |
| `seattrellis_buffer_free(buffer)` | Releases a returned buffer after the client copies its bytes. |

Free every response, including errors, using the unchanged pointer/length
pair. Do not mutate it or use the platform allocator. Destroying a session
does not free responses already returned to callers. `{NULL, 0}` reports
response-buffer allocation/quota failure.

Inputs must point to initialized, readable, immutable bytes in one live
allocation for the whole call. Null and excessive lengths are rejected before
reading. No C ABI can establish that an arbitrary non-null foreign address is
valid; dangling pointers, concurrent mutation and reads after free violate the
caller contract. Recoverable Rust panics are contained at the ABI boundary;
process aborts, including allocator exhaustion, are not recoverable responses.

## Bounded protocol

There are at most 16 live sessions, 8 MiB per request, 32 MiB per response,
256 outstanding response buffers and 128 MiB of outstanding response bytes
per process. Dispatch reserves a slot and 32 MiB of response capacity before
executing an operation, then accounts for the actual returned bytes. It can
therefore reject a call when less than 32 MiB of headroom remains; with no
outstanding responses, at most four dispatches run concurrently. Quota failure
does not execute the operation. Shared application limits also bound stored draft contexts.
These limits are independent of the HTTP transport's limits. Clients should
report size errors before replacing the current document.

```json
{
  "protocol_version": 1,
  "operation": "state",
  "payload": { "draft_id": "a-session-owned-draft-id" }
}
```

Successful responses contain `protocol_version`, `ok: true` and `result`.
Failures contain `ok: false` and `error` with `code`, `message` and numeric
`status`. Normal solver outcomes retain the shared seven-status vocabulary;
transport failures and cancellation can also be error envelopes. ABI major,
JSON protocol, editor protocol and saved-document versions are distinct.

| Operation | Payload/result |
| --- | --- |
| `generate` | Existing workbench or core request; returns status, feasibility, editor and candidate metadata. |
| `state` | `draft_id`; returns editor state and independent validation. |
| `command` | Existing editor envelope with protocol `"1.0"`, `base_revision`, action and operations. |
| `serialize` | Complete `class_source` and `draft_refs` containing IDs and revisions; returns the existing portable class document. |
| `open` | Existing class document; restores complete contexts and saved locks. |
| `repair` | `draft_id`, `base_revision` and optional `affected_students`; uses shared repair semantics. |
| `audit` | `draft_id`; returns the shared audit report. |
| `export` | `draft_id`, `format`, optional `options`; returns `filename`, `mime_type`, `base64` and warnings. |
| `delete` | `draft_id`; removes its editor and paired source. |

The bridge reuses the application and domain implementations. It does not
implement another scoring engine. Native exports use explicit options and
built-in defaults, without reading or writing global export preferences.
Platform clients own file pickers, sandbox permissions and writes. Complete
source data stays separate from the minimized editor projection.

## Verification and distribution

Rust integration tests exercise the exported ABI across generation, invalid
editing, lock-preserving save/reopen, audit, repair, export and deletion, with
session isolation, input bounds and ownership controls. The bridge also has
foreign-language smoke checks. The macOS CI job compiles the SwiftUI client,
runs its document tests and produces an unsigned preview bundle. An unsigned
CI artifact is not a signed/notarized release or a manual accessibility test.
