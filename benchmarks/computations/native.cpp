#include <array>
#include <bit>
#include <chrono>
#include <cinttypes>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <limits>
#include "kernels.h"

#define NOINLINE __attribute__((noinline))
using Kernel = std::int64_t (*)(std::int64_t, std::int64_t);
static volatile std::int64_t sink;

#define DECLARE_RUST(name) extern "C" std::int64_t rust_##name(std::int64_t, std::int64_t);
DECLARE_RUST(bind)
DECLARE_RUST(checked)
DECLARE_RUST(delayed)
DECLARE_RUST(array_for)
DECLARE_RUST(array_bind)
DECLARE_RUST(owned_capture)
DECLARE_RUST(std_option)
DECLARE_RUST(std_result)
DECLARE_RUST(std_option_owned)

static std::uint64_t mix(std::uint64_t value, std::uint64_t salt) {
    return (value ^ (value >> 13)) * UINT64_C(6364136223846793005) + salt;
}

template<int kind>
static NOINLINE std::int64_t cpp_loop(std::int64_t count, std::int64_t seed) {
    if (count < 0) std::abort();
    auto state = static_cast<std::uint64_t>(seed);
    for (auto remaining = count; remaining > 0; --remaining) {
        const auto salt = UINT64_C(1442695040888963407) + static_cast<std::uint64_t>(remaining);
        if constexpr (kind == 0) {
            state = mix(state, salt);
        } else if constexpr (kind == 1) {
            const auto first = state ^ salt;
            if ((state & 7) == 0) {
                state = first;
            } else {
                const auto second = mix(first, salt);
                state = (first & 3) == 0 ? second : second ^ salt;
            }
        } else if constexpr (kind == 2) {
            state = mix(state, salt) + (mix(state ^ 71, salt) + mix(state ^ 113, salt));
        } else if constexpr (kind == 3) {
            state = (state & 7) == 0 ? state ^ salt : mix(state ^ salt, salt);
        } else {
            const auto length = (state & 7) == 0 ? 0 : 12;
            state = mix(state, salt) ^ length;
        }
    }
    return std::bit_cast<std::int64_t>(state);
}

static NOINLINE std::int64_t cpp_array(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    auto *values = static_cast<std::uint64_t *>(std::malloc(count == 0 ? 1 : count * sizeof(std::uint64_t)));
    if (!values) std::abort();
    for (std::int64_t index = 0; index < count; ++index) {
        values[index] = mix(static_cast<std::uint64_t>(index), static_cast<std::uint64_t>(seed));
    }
    std::uint64_t total = 0;
    const auto scale = static_cast<std::uint64_t>(seed) | 1;
    for (std::int64_t index = 0; index < count; ++index) {
        const auto value = values[index];
        total += (value ^ (value >> 17)) * scale;
    }
    std::free(values);
    return std::bit_cast<std::int64_t>(total);
}

static NOINLINE std::int64_t cpp_owned(std::int64_t count, std::int64_t seed) {
    if (count < 0) std::abort();
    std::array<std::uint64_t, 256> values;
    for (std::size_t index = 0; index < values.size(); ++index) {
        values[index] = mix(index, static_cast<std::uint64_t>(seed));
    }
    auto state = static_cast<std::uint64_t>(seed);
    for (std::int64_t index = 0; index < count; ++index) state = mix(values[state & 255], state);
    return std::bit_cast<std::int64_t>(state);
}

#ifdef BASELINE
#include "before.h"
#endif

#ifdef TRACKING
static std::uint64_t allocations, bytes, live;
extern "C" void *tracked_alloc(std::uint64_t size) {
    auto *pointer = static_cast<std::uint64_t *>(std::malloc(size + 16));
    if (!pointer) std::abort();
    pointer[0] = size;
    pointer[1] = UINT64_C(0x51a110ca7e);
    ++allocations;
    bytes += size;
    live += size;
    return pointer + 2;
}
extern "C" void tracked_free(void *value) {
    if (!value) return;
    auto *pointer = static_cast<std::uint64_t *>(value) - 2;
    if (pointer[1] != UINT64_C(0x51a110ca7e) || live < pointer[0]) std::abort();
    pointer[1] = 0;
    live -= pointer[0];
    std::free(pointer);
}
#endif

struct Workload {
    const char *name;
    Kernel cpp, direct, computation, rust;
#ifdef BASELINE
    Kernel before;
#endif
    std::int64_t size;
};
#ifdef BASELINE
#define WORKLOAD(name, cpp, count) {#name, cpp, tz_direct_##name, tz_ce_##name, rust_##name, before_tz_ce_##name, count}
#else
#define WORKLOAD(name, cpp, count) {#name, cpp, tz_direct_##name, tz_ce_##name, rust_##name, count}
#endif

#ifndef TRACKING
static NOINLINE double measure(Kernel kernel, std::int64_t count, std::int64_t seed,
                               int repeats, std::int64_t expected, double &wall_ms) {
    volatile std::int64_t input_count = count, input_seed = seed;
    const auto wall_start = std::chrono::steady_clock::now();
    const auto start = std::clock();
    for (int index = 0; index < repeats; ++index) {
        const auto result = kernel(input_count, input_seed);
        sink = result;
        if (result != expected) {
            std::fputs("checksum mismatch during measurement\n", stderr);
            std::exit(1);
        }
    }
    const auto end = std::clock();
    wall_ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - wall_start).count() / repeats;
    if (start == static_cast<std::clock_t>(-1) || end < start) {
        std::fputs("CPU clock is unavailable\n", stderr);
        std::exit(1);
    }
    return static_cast<double>(end - start) * 1000.0 / CLOCKS_PER_SEC / repeats;
}
#endif

int main(int argc, char **argv) {
    const bool quick = argc == 2 && std::strcmp(argv[1], "--quick") == 0;
    double scale = 1.0;
    if (argc == 3 && std::strcmp(argv[1], "--scale") == 0) {
        char *end;
        scale = std::strtod(argv[2], &end);
        if (end == argv[2] || *end || !(scale > 0.0 && scale <= 10.0)) return 2;
    } else if (argc != 1 && !quick) return 2;
    const Workload workloads[] = {
        WORKLOAD(bind, cpp_loop<0>, quick ? 1024 : 2000000),
        WORKLOAD(checked, cpp_loop<1>, quick ? 1024 : 2000000),
        WORKLOAD(delayed, cpp_loop<2>, quick ? 256 : 500000),
        WORKLOAD(array_for, cpp_array, quick ? 512 : 8192),
        WORKLOAD(array_bind, cpp_array, quick ? 512 : 8192),
        WORKLOAD(owned_capture, cpp_owned, quick ? 128 : 8192),
        WORKLOAD(std_option, cpp_loop<3>, quick ? 1024 : 2000000),
        WORKLOAD(std_result, cpp_loop<1>, quick ? 1024 : 2000000),
        WORKLOAD(std_option_owned, cpp_loop<4>, quick ? 128 : 8192),
    };
        const int samples = (quick ? 1 : 3) *
    #ifdef BASELINE
        5;
    #else
        4;
    #endif
    std::printf("{\"samples\":%d,\"clock_ticks_per_second\":%ld,\"workloads\":[",
                samples, static_cast<long>(CLOCKS_PER_SEC));
    bool first = true;
    for (const auto &definition : workloads) {
        auto work = definition;
        if (!quick) work.size = static_cast<std::int64_t>(std::fmax(1.0, std::ceil(work.size * scale)));
        const Kernel kernels[] = {work.cpp, work.direct, work.computation, work.rust,
#ifdef BASELINE
            work.before,
#endif
        };
        const char *names[] = {"cpp", "direct", "computation", "rust",
#ifdef BASELINE
            "before",
#endif
        };
        constexpr auto variants = sizeof(kernels) / sizeof(kernels[0]);
        std::printf("%s{\"name\":\"%s\",\"size\":%" PRId64 ",\"checks\":[", first ? "" : ",", work.name, work.size);
        first = false;
        bool first_check = true;
        for (const auto size : {0, 1, 2, 17, 257}) {
            for (const std::int64_t seed : {
                -INT64_C(3), INT64_C(0), INT64_C(42),
                std::numeric_limits<std::int64_t>::min(), std::numeric_limits<std::int64_t>::max(),
            }) {
                const auto expected = work.cpp(size, seed);
                for (auto kernel : kernels) {
                    if (kernel(size, seed) != expected) {
                        std::fprintf(stderr, "%s: checksum mismatch at size=%d, seed=%" PRId64 "\n", work.name, size, seed);
                        return 1;
                    }
                }
                std::printf("%s{\"size\":%d,\"seed\":\"%" PRId64 "\",\"checksum\":\"%016" PRIx64 "\"}",
                            first_check ? "" : ",", size, seed, static_cast<std::uint64_t>(expected));
                first_check = false;
            }
        }
        std::fputs("],", stdout);
#ifdef TRACKING
        std::fputs("\"allocations\":{", stdout);
        for (std::size_t variant = 1; variant < variants; ++variant) {
            if (variant == 3) continue;
            allocations = bytes = 0;
            sink = kernels[variant](64, 42);
            if (live != 0) {
                std::fprintf(stderr, "%s %s: retained %" PRIu64 " bytes\n", work.name, names[variant], live);
                return 1;
            }
            std::printf("%s\"%s\":{\"calls\":%" PRIu64 ",\"bytes\":%" PRIu64 "}",
                        variant == 1 ? "" : ",", names[variant], allocations, bytes);
        }
        std::fputs("}", stdout);
#else
        int repeats[variants];
        for (std::size_t variant = 0; variant < variants; ++variant) {
            const auto expected = work.cpp(work.size, 40);
            double wall_ms;
            measure(kernels[variant], work.size, 40, 1, expected, wall_ms);
            const auto elapsed = measure(kernels[variant], work.size, 40, 1, expected, wall_ms);
            repeats[variant] = quick ? 1 : static_cast<int>(std::fmin(10000.0, std::ceil(20.0 / std::fmax(0.001, elapsed))));
        }
        std::fputs("\"raw\":[", stdout);
        for (int sample = 0; sample < samples; ++sample) {
            const auto expected = work.cpp(work.size, 42 + sample);
            double durations[variants], wall_durations[variants];
            for (std::size_t index = 0; index < variants; ++index) {
                const auto variant = (index + sample) % variants;
                durations[variant] = measure(kernels[variant], work.size, 42 + sample, repeats[variant], expected, wall_durations[variant]);
            }
            std::printf("%s{\"seed\":\"%d\",\"checksum\":\"%016" PRIx64 "\"", sample ? "," : "", 42 + sample, static_cast<std::uint64_t>(expected));
            for (std::size_t variant = 0; variant < variants; ++variant) {
                std::printf(",\"%s_ms\":%.9f,\"%s_wall_ms\":%.9f,\"%s_repeats\":%d", names[variant], durations[variant], names[variant], wall_durations[variant], names[variant], repeats[variant]);
            }
            std::fputs("}", stdout);
        }
        std::fputs("]", stdout);
#endif
        std::fputs("}", stdout);
    }
    std::puts("]}");
}
