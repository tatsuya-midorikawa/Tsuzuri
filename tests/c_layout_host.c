#undef NDEBUG
#include <assert.h>
#include <stdint.h>
#include <string.h>
#include "c_layout.h"

tz_handle_4Main_9TagHandle cl_make_handle(int64_t v) {
    return (tz_handle_4Main_9TagHandle)(uintptr_t)v;
}

int64_t cl_inspect_font(const tz_record_4Main_9SmallFont *f) {
    assert(f->height == -12);
    assert(f->italic == -128);
    assert(f->flags == 255);
    assert(f->weight == -32768);
    assert(f->code_page == 65535);
    assert(f->scale == 1.5f);
    assert(f->dpi == 96.25);
    assert(f->face[0] == 65 && f->face[1] == 66 && f->face[2] == 67 && f->face[3] == 68);
    return 1;
}

int32_t cl_mutate_font(tz_record_4Main_9SmallFont *f) {
    /* Fill entire struct with 0xAA first to prove Tsuzuri ignores padding bytes on copy-out. */
    tz_record_4Main_9SmallFont copy = *f;
    memset(f, 0xAA, sizeof(*f));
    copy.italic = 12;
    copy.code_page = 1234;
    copy.face[2] = 99;
    f->height = copy.height;
    f->italic = copy.italic;
    f->flags = copy.flags;
    f->weight = copy.weight;
    f->code_page = copy.code_page;
    f->scale = copy.scale;
    f->dpi = copy.dpi;
    for (int i = 0; i < 4; i++) f->face[i] = copy.face[i];
    return 1;
}

void cl_make_dlg(tz_record_4Main_7DlgMini *out, uint32_t style, uint16_t cdit, int16_t x, int16_t y) {
    memset(out, 0x55, sizeof(*out));
    out->style = style;
    out->cdit = cdit;
    out->x = x;
    out->y = y;
}

int64_t cl_check_packed(const tz_record_4Main_11PackedOuter *p) {
    assert(p->tag == 5);
    assert(p->inner.x == 100);
    assert(p->inner.y == 200000);
    return (int64_t)p->tag + (int64_t)p->inner.x + p->inner.y;
}

int64_t cl_check_handles(tz_record_4Main_11WithHandles *wh) {
    assert((int64_t)(uintptr_t)wh->primary == 111);
    assert(wh->secondary == NULL);
    assert(wh->tag == 9);
    int64_t sum = (int64_t)(uintptr_t)wh->primary + wh->tag;
    wh->secondary = (tz_handle_4Main_9TagHandle)(uintptr_t)222;
    wh->tag = 42;
    return sum;
}

static int64_t utf16_len(const uint16_t *s) {
    int64_t n = 0;
    while (s[n] != 0) n++;
    return n;
}

int64_t cl_invoke_phase2(const tz_record_4Main_11Phase2Input *in) {
    assert(in->cb != NULL);
    int32_t doubled = in->cb((int32_t)in->code);
    int64_t tlen = utf16_len(in->title);
    int64_t mlen = in->opt_menu ? utf16_len(in->opt_menu) : 0;
    return (int64_t)doubled + tlen + mlen + (int64_t)in->code;
}

int main(void) {
    tz_record_4Main_9SmallFont f;
    memset(&f, 0xCC, sizeof(f));
    f.height = 10;
    f.italic = -5;
    f.flags = 200;
    f.weight = -100;
    f.code_page = 500;
    f.scale = 1.0f;
    f.dpi = 72.0;
    f.face[0] = 1; f.face[1] = 2; f.face[2] = 3; f.face[3] = 4;
    assert(tz_sum_font(&f) == 10 - 5 + 200 - 100 + 500 + 1 + 4);
    assert(tz_run_all() == 1);
    return 0;
}
