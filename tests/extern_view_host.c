#undef NDEBUG
#include <assert.h>
#include <stdint.h>
#include <string.h>
#include "extern_view.h"

typedef struct {
    int64_t hwnd_from;
    uint64_t id_from;
    uint32_t code;
    uint8_t tail_pad[4];
} HostNmHdr;

typedef struct { int32_t x; int32_t y; } HostPoint;

typedef struct {
    HostPoint reserved;
    HostPoint max_size;
    HostPoint max_position;
    HostPoint min_track_size;
    HostPoint max_track_size;
} HostMinMaxInfo;

#pragma pack(push, 2)
typedef struct {
    int8_t tag;
    uint8_t pad1;
    uint16_t count;
    void *handle;
    void *opt_handle;
    int16_t vals[3];
} HostPackedView;
#pragma pack(pop)

int main(void) {
    HostNmHdr hdr;
    memset(&hdr, 0xAB, sizeof(hdr));
    hdr.hwnd_from = 1000;
    hdr.id_from = 250;
    hdr.code = 42;
    assert(tz_inspect_nmhdr((int64_t)(intptr_t)&hdr) == 1292);

    HostMinMaxInfo mmi;
    memset(&mmi, 0, sizeof(mmi));
    mmi.max_size.x = 800;
    mmi.max_size.y = 600;
    mmi.max_track_size.x = 1920;
    mmi.max_track_size.y = 1080;
    assert(tz_update_mmi((int64_t)(intptr_t)&mmi) == 900 + 240);
    assert(mmi.max_size.x == 900 && mmi.max_size.y == 800);
    assert(mmi.min_track_size.x == 320 && mmi.min_track_size.y == 240);
    assert(mmi.max_track_size.x == 1920 && mmi.max_track_size.y == 1080);

    HostPackedView pv;
    memset(&pv, 0x5A, sizeof(pv));
    pv.tag = -10;
    pv.count = 500;
    pv.handle = (void *)(uintptr_t)0x1234;
    pv.opt_handle = NULL;
    pv.vals[0] = 7; pv.vals[1] = 8; pv.vals[2] = 9;
    assert(tz_round_trip_packed((int64_t)(intptr_t)&pv) == -10 + 500 + 0 + 7 + 9);
    assert(pv.tag == -9);
    assert(pv.count == 510);
    assert(pv.handle == (void *)(uintptr_t)0x1234);
    assert(pv.opt_handle == (void *)(uintptr_t)0x1234);
    assert(pv.vals[0] == 9 && pv.vals[1] == 8 && pv.vals[2] == 7);
    /* Padding byte must be untouched by Tsuzuri field-by-field write! */
    assert(pv.pad1 == 0x5A);
    return 0;
}
