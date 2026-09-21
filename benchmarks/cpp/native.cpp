#include <bit>
#include <cinttypes>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <limits>
#include <memory>
#include "mix.h"
#include "kernels.h"

#define NOINLINE __attribute__((noinline))

using Kernel = std::int64_t (*)(std::int64_t, std::int64_t);
static volatile std::int64_t sink;

NOINLINE std::int64_t cpp_mix(std::int64_t iterations, std::int64_t seed) {
    if (iterations < 0) std::abort();
    auto state = static_cast<std::uint64_t>(seed);
    for (std::int64_t index = 0; index < iterations; ++index) {
        state = (state ^ (state >> 13)) * UINT64_C(6364136223846793005)
            + UINT64_C(1442695040888963407);
    }
    return std::bit_cast<std::int64_t>(state);
}

NOINLINE std::int64_t cpp_mandelbrot(std::int64_t size, std::int64_t seed) {
    if (size < 0 || size > 4096) std::abort();
    if (size == 0) return 0;
    const double dx = 3.0 / static_cast<double>(size);
    const double dy = 2.0 / static_cast<double>(size);
    const double offset = static_cast<double>(seed) * 0.000001;
    std::int64_t total = 0;
    for (std::int64_t index = 0; index < size * size; ++index) {
        const double cr = static_cast<double>(index % size) * dx - 2.0 + offset;
        const double ci = static_cast<double>(index / size) * dy - 1.0;
        double real = 0.0, imaginary = 0.0;
        std::int64_t count = 0;
        while (count < 256 && real * real + imaginary * imaginary <= 4.0) {
            const double next_real = real * real - imaginary * imaginary + cr;
            imaginary = 2.0 * real * imaginary + ci;
            real = next_real;
            ++count;
        }
        total += count;
    }
    return total;
}

NOINLINE std::int64_t cpp_array_sum(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    auto values = std::make_unique_for_overwrite<std::uint64_t[]>(
        static_cast<std::size_t>(count));
    for (std::int64_t index = 0; index < count; ++index) {
        values[index] = (static_cast<std::uint64_t>(index) ^ static_cast<std::uint64_t>(seed))
            * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
    }
    std::uint64_t total = 0;
    for (std::int64_t index = 0; index < count; ++index) total += values[index];
    return std::bit_cast<std::int64_t>(total);
}

static std::int64_t check(Kernel cpp, Kernel tsuzuri, std::int64_t size, std::int64_t seed) {
    const auto expected = cpp(size, seed);
    if (tsuzuri(size, seed) != expected) {
        std::fprintf(stderr, "checksum mismatch: size=%" PRId64 ", seed=%" PRId64 "\n", size, seed);
        std::exit(1);
    }
    sink = expected;
    return expected;
}

static NOINLINE double measure(Kernel function, std::int64_t size, std::int64_t seed,
                               std::int64_t &result) {
    volatile std::int64_t input_size = size, input_seed = seed;
    const auto start = std::clock();
    const auto value = function(input_size, input_seed);
    sink = value;
    const auto end = std::clock();
    if (start == static_cast<std::clock_t>(-1) || end == static_cast<std::clock_t>(-1) || end < start) {
        std::fputs("CPU clock is unavailable\n", stderr);
        std::exit(1);
    }
    result = value;
    return static_cast<double>(end - start) * 1000.0 / CLOCKS_PER_SEC;
}

int main(int argc, char **argv) {
    if (argc > 2 || (argc == 2 && std::strcmp(argv[1], "--quick") != 0)) {
        std::fputs("usage: benchmark [--quick]\n", stderr);
        return 2;
    }
    const bool quick = argc == 2;
    const struct {
        const char *name;
        Kernel cpp, tsuzuri;
        std::int64_t size;
    } workloads[] = {
        {"integer_mix", cpp_mix, tz_mix, quick ? 10000 : 20000000},
        {"mandelbrot", cpp_mandelbrot, tz_mandelbrot, quick ? 16 : 768},
        {"array_sum", cpp_array_sum, tz_array_sum, quick ? 1024 : 8000000},
    };
    constexpr std::int64_t sizes[] = {0, 1, 2, 7, 16};
    constexpr std::int64_t seeds[] = {
        -3, 0, 42, std::numeric_limits<std::int64_t>::min(), std::numeric_limits<std::int64_t>::max(),
    };
    std::printf("{\"samples\":10,\"clock_ticks_per_second\":%ld,\"workloads\":[",
                static_cast<long>(CLOCKS_PER_SEC));
    bool first_workload = true;
    for (const auto &workload : workloads) {
        std::printf("%s{\"name\":\"%s\",\"size\":%" PRId64 ",\"checks\":[",
                    first_workload ? "" : ",", workload.name, workload.size);
        first_workload = false;
        bool first_check = true;
        for (const auto size : sizes) {
            for (const auto seed : seeds) {
                const auto result = check(workload.cpp, workload.tsuzuri, size, seed);
                std::printf("%s{\"size\":%" PRId64 ",\"seed\":\"%" PRId64
                            "\",\"checksum\":\"%016" PRIx64 "\"}",
                            first_check ? "" : ",", size, seed, static_cast<std::uint64_t>(result));
                first_check = false;
            }
        }
        check(workload.cpp, workload.tsuzuri, workload.size, 41);
        check(workload.tsuzuri, workload.cpp, workload.size, 40);
        std::fputs("],\"raw\":[", stdout);
        for (int sample = 0; sample < 10; ++sample) {
            std::int64_t cpp_result, tsuzuri_result;
            double cpp_ms, tsuzuri_ms;
            if (sample % 2 == 0) {
                cpp_ms = measure(workload.cpp, workload.size, 42 + sample, cpp_result);
                tsuzuri_ms = measure(workload.tsuzuri, workload.size, 42 + sample, tsuzuri_result);
            } else {
                tsuzuri_ms = measure(workload.tsuzuri, workload.size, 42 + sample, tsuzuri_result);
                cpp_ms = measure(workload.cpp, workload.size, 42 + sample, cpp_result);
            }
            if (cpp_result != tsuzuri_result) {
                std::fprintf(stderr, "%s: sample %d checksum mismatch\n", workload.name, sample);
                return 1;
            }
            std::printf("%s{\"cpp_ms\":%.6f,\"tsuzuri_ms\":%.6f,\"checksum\":\"%016" PRIx64 "\"}",
                        sample ? "," : "", cpp_ms, tsuzuri_ms, static_cast<std::uint64_t>(cpp_result));
        }
        std::fputs("]}", stdout);
    }
    std::puts("]}");
}
