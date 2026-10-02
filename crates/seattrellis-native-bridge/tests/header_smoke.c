#include "seattrellis.h"
#include <assert.h>
#include <stddef.h>
#include <string.h>

_Static_assert(offsetof(SeattrellisBuffer, data) == 0, "buffer data offset");
_Static_assert(offsetof(SeattrellisBuffer, len) == sizeof(void *), "buffer len offset");
_Static_assert(sizeof(SeattrellisBuffer) == sizeof(void *) + sizeof(size_t), "buffer layout");

int main(void) {
    const char request[] = "{\"protocol_version\":1,\"operation\":\"generate\",\"payload\":{\"api_version\":2,\"student_count\":2,\"seat_positions\":[[0,0],[1,0]]}}";
    assert(seattrellis_abi_version() == 1);
    uint64_t session = seattrellis_session_create();
    assert(session != 0);
    SeattrellisBuffer result = seattrellis_session_dispatch(session, (const uint8_t *)request, strlen(request));
    assert(result.data != NULL && result.len > 0);
    /* Results are not NUL-terminated: compare bounded slices only. */
    const char needle[] = "\"status\":\"Solved\"";
    int solved = 0;
    for (size_t offset = 0; offset + sizeof(needle) - 1 <= result.len; ++offset) {
        if (memcmp(result.data + offset, needle, sizeof(needle) - 1) == 0) {
            solved = 1;
            break;
        }
    }
    assert(solved);
    seattrellis_buffer_free(result);
    assert(seattrellis_session_cancel(session) == 0);
    seattrellis_session_destroy(session);
    assert(seattrellis_session_cancel(session) == -1);
    return 0;
}
