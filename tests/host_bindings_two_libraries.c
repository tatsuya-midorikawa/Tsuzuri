/* The headers of two Tsuzuri libraries in one translation unit (tests/host_bindings.mjs). liba and libb
   both export the record of the package `common` they depend on; libb is built with --trap-mode return.
   The suite compiles this file as C11, as C++17, and with TSUZURI_CPP_BINDINGS as C++20 with the
   C++ bindings of both libraries, and links both shared libraries. */
#include <stdio.h>
#include <string.h>

#include "liba.h"
#include "libb.h"
#ifdef TSUZURI_CPP_BINDINGS
#include "liba.hpp"
#include "libb.hpp"
#endif

#ifdef _WIN32
#include <windows.h>
typedef void (*tz_dll_free_fn)(void *);
static void free_from_dll(const wchar_t *dll_name, void *ptr) {
    HMODULE mod = GetModuleHandleW(dll_name);
    FARPROC proc = mod ? GetProcAddress(mod, "tsuzuri_free") : NULL;
    if (proc) {
        ((tz_dll_free_fn)(uintptr_t)proc)(ptr);
    } else {
        tsuzuri_free(ptr);
    }
}
#endif

static int failures = 0;

#define CHECK(condition)                                                            \
    do {                                                                            \
        if (!(condition)) {                                                         \
            fprintf(stderr, "%s:%d: failed: %s\n", __FILE__, __LINE__, #condition); \
            ++failures;                                                             \
        }                                                                           \
    } while (0)

int main(void) {
    /* The record of the shared dependency is one type for both libraries. */
    tz_record_6Common_3Geo_5Point point;
    tz_origin(&point, 3);
    CHECK(point.x == 3.0 && point.y == 0.5);
    CHECK(tz_coordinate_sum(&point) == 3.5);

    /* The buffer descriptors of both, freed through each library's own tsuzuri_free. */
    const uint16_t name[] = {0x0054, 0x007A, 0xD83D, 0xDE00};
    tsuzuri_string_buffer greeting;
    tz_greet(&greeting, name, 4);
    CHECK(greeting.len == 4 && memcmp(greeting.ptr, name, sizeof name) == 0);
#ifdef _WIN32
    free_from_dll(L"liba.dll", greeting.ptr);
#else
    tsuzuri_free(greeting.ptr);
#endif
    const uint8_t text[] = {0x6F, 0x6B, 0xC3, 0xA9};
    tsuzuri_utf8string_buffer copy;
    tz_echo(&copy, text, 4);
    CHECK(copy.len == 4 && memcmp(copy.ptr, text, sizeof text) == 0);
#ifdef _WIN32
    free_from_dll(L"libb.dll", copy.ptr);
#else
    tsuzuri_free(copy.ptr);
#endif
    const int64_t values[] = {1, 2, 39};
    CHECK(tz_total(values, 3) == 42);

#ifndef TSUZURI_NO_TRAP_MODE
    /* The trap record of libb's --trap-mode return. */
    tsuzuri_trap_info trap;
    int64_t result = 0;
    CHECK(tsuzuri_try_total(&trap, &result, values, 3) == 0 && result == 42);
    CHECK(tsuzuri_try_divide(&trap, &result, 7, 0) == 1 && trap.kind == 1);
    CHECK(tsuzuri_try_divide(&trap, &result, 84, 2) == 0 && result == 42);
#else
    CHECK(tz_divide(84, 2) == 42);
#endif

#ifdef TSUZURI_CPP_BINDINGS
    const tz_record_6Common_3Geo_5Point made = tsuzuri::libb::origin(4);
    CHECK(tsuzuri::liba::coordinate_sum(made) == 4.5);
    const std::int64_t more[] = {40, 2};
    CHECK(tsuzuri::libb::total(more) == 42);
    CHECK(tsuzuri::liba::greet(u"Tz").view() == u"Tz");
    CHECK(tsuzuri::libb::echo("ok").view() == "ok");
#ifndef TSUZURI_NO_TRAP_MODE
    bool trapped = false;
    try {
        tsuzuri::libb::divide(1, 0);
    } catch (const tsuzuri::libb::trap_error &error) {
        trapped = error.kind() == 1;
    }
    CHECK(trapped);
#else
    CHECK(tsuzuri::libb::divide(84, 2) == 42);
#endif
#endif

    if (failures == 0) puts("two libraries passed");
    return failures == 0 ? 0 : 1;
}
