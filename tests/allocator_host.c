/* The host side of tests/allocator.mjs (F13): it defines the three --allocator host functions,
   checks every call against the block it recorded, and runs the host_abi fixture's exports.
   Arguments: none (run and report the counts), "oom" (allocation fails), "misaligned"
   (allocation returns an address that is not a multiple of 16). */
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "lib.h"

#define MAGIC UINT64_C(0x4a110c470b)

static enum { NORMAL, OOM, MISALIGNED } mode;
static _Atomic uint64_t allocations, frees, reallocations, live;
static _Atomic uint64_t last_size;

/* Each block is a malloc'ed area with the host's own 16-byte prefix (size, magic). */
void *tsuzuri_host_alloc(uint64_t size, uint64_t align) {
    assert(align == 16 && size >= 16);
    last_size = size;
    if (mode == OOM) return NULL;
    uint64_t *block = malloc((size_t)size + 32);
    assert(block && ((uintptr_t)block & 15) == 0);
    block[0] = size;
    block[1] = MAGIC;
    ++allocations;
    live += size;
    return mode == MISALIGNED ? (char *)(block + 2) + 8 : (void *)(block + 2);
}

void tsuzuri_host_free(void *pointer, uint64_t size, uint64_t align) {
    assert(pointer && align == 16 && ((uintptr_t)pointer & 15) == 0);
    uint64_t *block = (uint64_t *)pointer - 2;
    assert(block[1] == MAGIC && block[0] == size && live >= size);
    block[1] = 0;
    ++frees;
    live -= size;
    free(block);
}

void *tsuzuri_host_realloc(void *pointer, uint64_t old_size, uint64_t new_size, uint64_t align) {
    assert(pointer && align == 16 && new_size >= 16 && ((uintptr_t)pointer & 15) == 0);
    uint64_t *block = (uint64_t *)pointer - 2;
    assert(block[1] == MAGIC && block[0] == old_size);
    uint64_t *moved = realloc(block, (size_t)new_size + 32);
    assert(moved && ((uintptr_t)moved & 15) == 0);
    moved[0] = new_size;
    ++reallocations;
    live += new_size;
    live -= old_size;
    return moved + 2;
}

int main(int argc, char **argv) {
    if (argc > 1) {
        mode = strcmp(argv[1], "oom") == 0 ? OOM : MISALIGNED;
        tsuzuri_ubyte_buffer bytes;
        tz_make_bytes(&bytes, 16);
        return 0;
    }
    int64_t values[] = { 1, 2, 3 };
    uint16_t text[] = { 65, 0, 0xD800, 0xD83D, 0xDE00 };
    assert(tz_sum(values, 3) == 6);
    tsuzuri_i64_buffer copied;
    tz_copy_values(&copied, values, 3);
    assert(copied.len == 3 && copied.ptr[2] == 3 && ((uintptr_t)copied.ptr & 15) == 0);
    tsuzuri_free(copied.ptr);
    tsuzuri_string_buffer copied_text;
    tz_copy_text(&copied_text, text, 5);
    assert(copied_text.len == 5 && memcmp(copied_text.ptr, text, sizeof(text)) == 0);
    tsuzuri_free(copied_text.ptr);
    tsuzuri_ubyte_buffer bytes;
    for (int index = 0; index < 1024; ++index) {
        tz_make_bytes(&bytes, 257);
        assert(bytes.len == 257 && bytes.ptr[255] == 255 && bytes.ptr[256] == 0);
        tsuzuri_free(bytes.ptr);
        assert(live == 0);
    }
    /* A zero-byte request is one byte plus the 16-byte header; freeing null calls nothing. */
    void *empty = tsuzuri_alloc(0);
    assert(empty && last_size == 17);
    uint64_t before = frees;
    tsuzuri_free(empty);
    tsuzuri_free(NULL);
    assert(frees == before + 1);
    assert(allocations > 0 && allocations == frees && live == 0);
    printf("allocations=%llu frees=%llu live=%llu\n", (unsigned long long)allocations,
           (unsigned long long)frees, (unsigned long long)live);
    return 0;
}
