// Checks the C++ bindings of tests/fixtures/bindings_native (tests/host_bindings.mjs). Built twice:
// against native.hpp and the library of --allocator counting, and with -DTRAP_MODE against
// native_trap.hpp and the library of --trap-mode return.
#undef NDEBUG
#include <cassert>
#include <cstdint>
#include <cstring>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#ifdef TRAP_MODE
#include "native_trap.hpp"
namespace api = tsuzuri::native_trap;
#else
#include "native.hpp"
namespace api = tsuzuri::native;
#endif

template <class Call>
static bool throws_invalid_argument(Call call) {
    try {
        call();
    } catch (const std::invalid_argument &) {
        return true;
    }
    return false;
}

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    assert(api::add(40, 2) == 42);
    assert(api::add_u64(UINT64_MAX, 0) == UINT64_MAX);
    assert(api::widen(-128, 65535, 4294967295u) == -128 + 65535 + 4294967295LL);
    assert(api::narrow(300) == 300 - 256 && api::narrow(-129) == 127);
    assert(api::negate(true) == false && api::negate(false) == true);
    assert(api::twice32(0.1f) == 0.1f * 2);
    const std::vector<double> halves{0.5, 0.25};
    assert(api::sum_float(halves) == 0.75 && api::sum_float({}) == 0.0);
    const std::uint8_t bytes[] = {1, 2, 255};
    assert(api::checksum(bytes) == 258);
    {
        auto made = api::make_bytes(4);
        assert(made.size() == 4 && made[0] == 0 && made[3] == 3);
        assert(api::make_bytes(0).empty());
        const std::int64_t values[] = {1, -2, INT64_MAX};
        auto copy = api::copy_values(values);
        assert(copy.size() == 3 && copy.data() != values && copy[2] == INT64_MAX);
        const std::u16string text = u"a\xD800" u"b\U0001F600";
        auto copied = api::copy_text(text);
        assert(copied.view() == text);
        auto utf8 = api::copy_utf8("\xC3\xA9\xF0\x9F\x98\x80");
        assert(utf8.view() == "\xC3\xA9\xF0\x9F\x98\x80");
        assert(throws_invalid_argument([] { (void)api::copy_utf8("\xED\xA0\x80"); }));
        assert(throws_invalid_argument([] { (void)api::copy_utf8("\xC0\x80"); }));
        auto scaled = api::scaled(halves, 4.0);
        assert(scaled.size() == 2 && scaled[0] == 2.0 && scaled[1] == 1.0);
        // Moving a buffer moves its ownership; the source frees nothing.
        auto moved = std::move(scaled);
        assert(moved.size() == 2);
    }
    tz_record_4Main_6Sample sample{{-1, 65535, 1}, 1.5};
    const auto updated = api::update(sample);
    assert(updated.flags.tiny == -1 && updated.flags.wide == 65535 && updated.flags.flag == 0 && updated.amount == 2.5);
    assert(api::area({2.0, 3.5}) == 7.0);
    const tz_record_4Main_6Window window{{1, 2, 3}, 2.0, 7};
    assert(api::window_total(window) == 19.0);
    const auto made = api::make_window(5);
    assert(made.values[0] == 5 && made.values[1] == 10 && made.values[2] == -5 && made.scale == 0.5 && made.mark == 255);
    assert(api::counters(10) == 30);
    auto *opaque = reinterpret_cast<tz_handle_4Main_7Counter>(static_cast<std::uintptr_t>(0x1234));
    assert(api::pass_through(opaque) == opaque);
    // A borrowed handle reaches the host as the pointer itself: here, an int64 counter cell.
    std::int64_t cell = 41;
    assert(api::peek(reinterpret_cast<tz_handle_4Main_7Counter>(&cell)) == 41);
    api::check(5);
#ifdef TRAP_MODE
    try {
        (void)api::divide(1, 0);
        assert(false);
    } catch (const api::trap_error &error) {
        assert(error.kind() == 1 && std::string(error.kind_name()) == "integer division by zero");
        assert(std::string(error.what()).starts_with("trap: integer division by zero (site "));
    }
    assert(api::divide(-7, 2) == -3);
    try {
        api::check(-1);
        assert(false);
    } catch (const api::trap_error &error) {
        assert(std::string(error.kind_name()) == "assertion failed");
    }
    assert(api::copy_text(u"after").view() == u"after");
#else
    if (argc > 1) {
        // The library was built without --trap-mode return: a trap ends the process.
        (void)api::divide(1, 0);
        return 0;
    }
    tsuzuri_allocation_stats stats{};
    tsuzuri_alloc_stats(&stats);
    assert(stats.live_bytes == 0 && stats.allocations == stats.frees);
#endif
    return 0;
}
