#ifndef SEATTRELLIS_H
#define SEATTRELLIS_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32) && !defined(SEATTRELLIS_STATIC)
#define SEATTRELLIS_API __declspec(dllimport)
#else
#define SEATTRELLIS_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct SeattrellisBuffer {
    uint8_t *data;
    size_t len;
} SeattrellisBuffer;

/* ABI 1: UTF-8 JSON, no trailing NUL, request <= 8 MiB, response <= 32 MiB.
 * Copy response bytes before seattrellis_buffer_free; never mutate/free them
 * with a platform allocator. Return {NULL,0} means response-buffer quota was
 * exhausted (256 buffers / 128 MiB process-wide, including each active call's
 * reserved 32 MiB maximum response). Quota is checked before execution: quota
 * rejection does not run the operation. Free every returned buffer,
 * including errors. Session destruction does not free outstanding responses.
 * Non-null input must point to len initialized, live, readable, immutable bytes
 * in one allocation for the entire call. The ABI cannot validate arbitrary
 * dangling/foreign addresses. File access belongs to the platform client.
 * Use cdecl on Windows; size_t follows the target architecture. */
SEATTRELLIS_API uint32_t seattrellis_abi_version(void);
/* Maximum 16 live sessions. IDs are opaque, nonzero and never recycled. */
SEATTRELLIS_API uint64_t seattrellis_session_create(void);
/* Thread-safe, does not wait for dispatch; cancels an active call. */
SEATTRELLIS_API void seattrellis_session_destroy(uint64_t session);
/* Synchronous; use a background thread. Same-session overlap returns busy. */
SEATTRELLIS_API SeattrellisBuffer seattrellis_session_dispatch(
    uint64_t session, const uint8_t *input, size_t len);
/* Non-blocking: 1 active cancellation requested; 0 idle; -1 invalid handle.
 * Each dispatch has a fresh cancellation control. Cancel before dispatch is
 * idle and does not cancel the next call: clients must handle queued work.
 * Cancellation racing an already committed operation can return success;
 * clients must reconcile that result rather than assume it rolled back. */
SEATTRELLIS_API int32_t seattrellis_session_cancel(uint64_t session);
SEATTRELLIS_API void seattrellis_buffer_free(SeattrellisBuffer buffer);

#ifdef __cplusplus
}
#endif
#endif
