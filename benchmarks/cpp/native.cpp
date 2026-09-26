#include <array>
#include <atomic>
#include <bit>
#include <chrono>
#include <charconv>
#include <codecvt>
#include <cinttypes>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <limits>
#include <locale>
#include <memory>
#include <string>
#include <thread>
#include <vector>
#include "mix.h"
#include "kernels.h"

#define NOINLINE __attribute__((noinline))

extern "C" std::int64_t rust_mix(std::int64_t, std::int64_t);
extern "C" std::int64_t rust_mandelbrot(std::int64_t, std::int64_t);
extern "C" std::int64_t rust_array_sum(std::int64_t, std::int64_t);
#define DECLARE_RUST(name) extern "C" std::int64_t rust_##name(std::int64_t, std::int64_t);
DECLARE_RUST(utf16_scan)
DECLARE_RUST(utf16_compare)
DECLARE_RUST(utf16_validate)
DECLARE_RUST(utf8_roundtrip)
DECLARE_RUST(format_parse)
DECLARE_RUST(math_intrinsics)
DECLARE_RUST(task_sequence)
DECLARE_RUST(task_sequential)
DECLARE_RUST(task_parallel)

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

static std::u16string make_text(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    if (count == 0) return {};
    std::u16string text((seed & 1) == 0 ? u"Az09-_ \n" : u"A\0\u03A9\U0001F600\u4E2Dz\n", 8);
    while (text.size() < static_cast<std::size_t>(count)) {
        auto copy = text;
        text = copy + text;
    }
    return text;
}

static std::int64_t text_sum(const std::u16string &text) {
    std::int64_t total = 0;
    for (const auto unit : text) total += unit;
    return total;
}

NOINLINE std::int64_t cpp_utf16_scan(std::int64_t count, std::int64_t seed) {
    return text_sum(make_text(count, seed));
}

NOINLINE std::int64_t cpp_utf16_compare(std::int64_t count, std::int64_t seed) {
    const auto text = make_text(count, seed);
    const auto copy = text;
    const auto left = copy + ((seed & 2) == 0 ? u"a" : u"b");
    const auto right = text + ((seed & 4) == 0 ? u"a" : u"b");
    return left == right ? 1 : left < right ? 2 : 4;
}

static bool well_formed(const std::u16string &text) {
    for (std::size_t index = 0; index < text.size(); ++index) {
        const auto unit = text[index];
        if (unit >= 0xD800 && unit <= 0xDBFF) {
            if (index + 1 == text.size() || text[index + 1] < 0xDC00 || text[index + 1] > 0xDFFF) return false;
            ++index;
        } else if (unit >= 0xDC00 && unit <= 0xDFFF) return false;
    }
    return true;
}

NOINLINE std::int64_t cpp_utf16_validate(std::int64_t count, std::int64_t seed) {
    auto text = make_text(count, seed);
    if ((seed & 8) != 0) text.push_back(0xD800);
    auto repaired = text;
    for (std::size_t index = 0; index < repaired.size(); ++index) {
        const auto unit = repaired[index];
        if (unit >= 0xD800 && unit <= 0xDBFF) {
            if (index + 1 < repaired.size() && repaired[index + 1] >= 0xDC00 && repaired[index + 1] <= 0xDFFF) ++index;
            else repaired[index] = 0xFFFD;
        } else if (unit >= 0xDC00 && unit <= 0xDFFF) repaired[index] = 0xFFFD;
    }
    return text_sum(repaired) + well_formed(text);
}

NOINLINE std::int64_t cpp_utf8_roundtrip(std::int64_t count, std::int64_t seed) {
    const auto text = make_text(count, seed);
    std::wstring_convert<std::codecvt_utf8_utf16<char16_t>, char16_t> converter;
    const auto bytes = converter.to_bytes(text.data(), text.data() + text.size());
    const auto restored = converter.from_bytes(bytes.data(), bytes.data() + bytes.size());
    if (restored != text) std::abort();
    return text_sum(restored) + static_cast<std::int64_t>(bytes.size());
}

NOINLINE std::int64_t cpp_format_parse(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    auto state = static_cast<std::uint64_t>(seed);
    for (std::int64_t index = 0; index < count; ++index) {
        state = (state ^ (state >> 13)) * UINT64_C(6364136223846793005) + static_cast<std::uint64_t>(index) + UINT64_C(1442695040888963407);
        std::array<char, 32> buffer;
        const auto formatted = std::to_chars(buffer.data(), buffer.data() + buffer.size(), state);
        if (formatted.ec != std::errc()) std::abort();
        const std::u16string text(buffer.data(), formatted.ptr);
        for (std::size_t offset = 0; offset < text.size(); ++offset) buffer[offset] = static_cast<char>(text[offset]);
        const auto parsed = std::from_chars(buffer.data(), buffer.data() + text.size(), state);
        if (parsed.ec != std::errc() || parsed.ptr != buffer.data() + text.size()) std::abort();
        state ^= text.size();
    }
    return std::bit_cast<std::int64_t>(state);
}

NOINLINE std::int64_t cpp_math_intrinsics(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    double state = static_cast<double>((seed & 65535) + 1) / 16.0;
    for (std::int64_t index = 0; index < count; ++index) {
        state = std::sqrt(std::fabs(state) + static_cast<double>(index & 255));
        state = std::floor(state * 16.0) / 16.0 + std::ceil(state) / 1024.0;
    }
    return static_cast<std::int64_t>(state * 1048576.0);
}

NOINLINE std::int64_t cpp_task_sequence(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    auto state = static_cast<std::uint64_t>(seed);
    for (std::int64_t index = 0; index < count; ++index) {
        const auto task = [state] { return state; };
        const auto value = task();
        state = (value ^ (value >> 13)) * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
    }
    return std::bit_cast<std::int64_t>(state);
}

NOINLINE std::int64_t cpp_task_sequential(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    std::uint64_t total = 0;
    for (int index = 0; index < 16; ++index) total += static_cast<std::uint64_t>(cpp_mix(count, seed ^ index));
    return std::bit_cast<std::int64_t>(total);
}

NOINLINE std::int64_t cpp_task_parallel(std::int64_t count, std::int64_t seed) {
    if (count < 0 || count > 100000000) std::abort();
    static const unsigned parallelism = std::min(16u, std::max(1u, std::thread::hardware_concurrency()));
    std::array<std::uint64_t, 16> results;
    std::atomic<unsigned> next{0};
    const auto work = [&] {
        for (;;) {
            const auto index = next.fetch_add(1, std::memory_order_relaxed);
            if (index >= results.size()) return;
            results[index] = static_cast<std::uint64_t>(cpp_mix(count, seed ^ index));
        }
    };
    std::vector<std::thread> workers;
    workers.reserve(parallelism - 1);
    for (unsigned index = 1; index < parallelism; ++index) workers.emplace_back(work);
    work();
    for (auto &worker : workers) worker.join();
    std::uint64_t total = 0;
    for (const auto value : results) total += value;
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
                               std::int64_t &result, double &wall_ms, int repeats = 1) {
    volatile std::int64_t input_size = size, input_seed = seed;
    const auto wall_start = std::chrono::steady_clock::now();
    const auto start = std::clock();
    std::int64_t value = 0;
    for (int repeat = 0; repeat < repeats; ++repeat) {
        value = function(input_size, input_seed);
        sink = value;
    }
    const auto end = std::clock();
    wall_ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - wall_start).count() / repeats;
    if (start == static_cast<std::clock_t>(-1) || end == static_cast<std::clock_t>(-1) || end < start) {
        std::fputs("CPU clock is unavailable\n", stderr);
        std::exit(1);
    }
    result = value;
    return static_cast<double>(end - start) * 1000.0 / CLOCKS_PER_SEC / repeats;
}

int main(int argc, char **argv) {
    const bool quick = argc == 2 && std::strcmp(argv[1], "--quick") == 0;
    double scale = 1.0;
    if (argc == 3 && std::strcmp(argv[1], "--scale") == 0) {
        char *end;
        scale = std::strtod(argv[2], &end);
        if (end == argv[2] || *end || !(scale > 0.0 && scale <= 10.0)) return 2;
    } else if (argc != 1 && !quick) return 2;
    const struct {
        const char *name;
        Kernel cpp, tsuzuri, rust;
        std::int64_t size;
    } workloads[] = {
        {"integer_mix", cpp_mix, tz_mix, rust_mix, quick ? 10000 : 20000000},
        {"mandelbrot", cpp_mandelbrot, tz_mandelbrot, rust_mandelbrot, quick ? 16 : 768},
        {"array_sum", cpp_array_sum, tz_array_sum, rust_array_sum, quick ? 1024 : 8000000},
        {"utf16_scan", cpp_utf16_scan, tz_utf16_scan, rust_utf16_scan, quick ? 64 : 262144},
        {"utf16_compare", cpp_utf16_compare, tz_utf16_compare, rust_utf16_compare, quick ? 64 : 262144},
        {"utf16_validate", cpp_utf16_validate, tz_utf16_validate, rust_utf16_validate, quick ? 64 : 262144},
        {"utf8_roundtrip", cpp_utf8_roundtrip, tz_utf8_roundtrip, rust_utf8_roundtrip, quick ? 64 : 262144},
        {"format_parse", cpp_format_parse, tz_format_parse, rust_format_parse, quick ? 16 : 50000},
        {"math_intrinsics", cpp_math_intrinsics, tz_math_intrinsics, rust_math_intrinsics, quick ? 1024 : 500000},
        {"task_sequence", cpp_task_sequence, tz_task_sequence, rust_task_sequence, quick ? 1024 : 500000},
        {"task_sequential", cpp_task_sequential, tz_task_sequential, rust_task_sequential, quick ? 128 : 500000},
        {"task_parallel", cpp_task_parallel, tz_task_parallel, rust_task_parallel, quick ? 128 : 500000},
    };
    constexpr std::int64_t sizes[] = {0, 1, 2, 7, 16};
    constexpr std::int64_t seeds[] = {
        -3, 0, 42, std::numeric_limits<std::int64_t>::min(), std::numeric_limits<std::int64_t>::max(),
    };
    std::printf("{\"samples\":12,\"clock_ticks_per_second\":%ld,\"workloads\":[",
                static_cast<long>(CLOCKS_PER_SEC));
    bool first_workload = true;
    for (const auto &definition : workloads) {
        auto workload = definition;
        if (!quick) workload.size = static_cast<std::int64_t>(std::fmax(1.0, std::ceil(workload.size *
            (std::strcmp(workload.name, "mandelbrot") == 0 ? std::sqrt(scale) : scale))));
        std::printf("%s{\"name\":\"%s\",\"size\":%" PRId64 ",\"checks\":[",
                    first_workload ? "" : ",", workload.name, workload.size);
        first_workload = false;
        bool first_check = true;
        for (const auto size : sizes) {
            for (const auto seed : seeds) {
                const auto result = check(workload.cpp, workload.tsuzuri, size, seed);
                check(workload.cpp, workload.rust, size, seed);
                std::printf("%s{\"size\":%" PRId64 ",\"seed\":\"%" PRId64
                            "\",\"checksum\":\"%016" PRIx64 "\"}",
                            first_check ? "" : ",", size, seed, static_cast<std::uint64_t>(result));
                first_check = false;
            }
        }
        check(workload.cpp, workload.tsuzuri, workload.size, 41);
        check(workload.rust, workload.cpp, workload.size, 41);
        check(workload.tsuzuri, workload.cpp, workload.size, 40);
        check(workload.rust, workload.tsuzuri, workload.size, 40);
        std::fputs("],\"raw\":[", stdout);
        constexpr int orders[][3] = {{0, 1, 2}, {1, 2, 0}, {2, 0, 1},
                                     {2, 1, 0}, {1, 0, 2}, {0, 2, 1}};
        const Kernel functions[] = {workload.cpp, workload.tsuzuri, workload.rust};
        int repeats[3];
        for (int variant = 0; variant < 3; ++variant) {
            std::int64_t result;
            double wall_ms;
            measure(functions[variant], workload.size, 40, result, wall_ms);
            repeats[variant] = quick ? 1 : static_cast<int>(std::fmin(10000.0, std::ceil(20.0 / std::fmax(0.001, wall_ms))));
        }
        for (int sample = 0; sample < 12; ++sample) {
            std::int64_t results[3];
            double elapsed[3], wall_elapsed[3];
            for (const auto implementation : orders[sample % 6]) {
                elapsed[implementation] = measure(functions[implementation], workload.size,
                                                  42 + sample, results[implementation], wall_elapsed[implementation], repeats[implementation]);
            }
            if (results[0] != results[1] || results[0] != results[2]) {
                std::fprintf(stderr, "%s: sample %d checksum mismatch\n", workload.name, sample);
                return 1;
            }
            std::printf("%s{\"cpp_ms\":%.6f,\"tsuzuri_ms\":%.6f,\"rust_ms\":%.6f,"
                        "\"cpp_wall_ms\":%.9f,\"tsuzuri_wall_ms\":%.9f,\"rust_wall_ms\":%.9f,"
                        "\"cpp_repeats\":%d,\"tsuzuri_repeats\":%d,\"rust_repeats\":%d,"
                        "\"seed\":\"%d\",\"checksum\":\"%016" PRIx64 "\"}", sample ? "," : "",
                        elapsed[0], elapsed[1], elapsed[2], wall_elapsed[0], wall_elapsed[1], wall_elapsed[2],
                        repeats[0], repeats[1], repeats[2],
                        42 + sample, static_cast<std::uint64_t>(results[0]));
        }
        std::fputs("]}", stdout);
    }
    std::puts("]}");
}
