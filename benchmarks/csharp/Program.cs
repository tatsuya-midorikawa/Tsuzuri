using System.Diagnostics;
using System.Globalization;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

static class Program
{
    static long inputSize, inputSeed, sink;

    static double Measure(string family, string name, long size, ulong seed, ulong expected, int repeats)
    {
        inputSize = size;
        inputSeed = unchecked((long)seed);
        long start = Stopwatch.GetTimestamp();
        for (int repeat = 0; repeat < repeats; ++repeat)
        {
            ulong result = Kernels.Run(family, name, Volatile.Read(ref inputSize), unchecked((ulong)Volatile.Read(ref inputSeed)));
            Volatile.Write(ref sink, unchecked((long)result));
            if (result != expected) throw new InvalidOperationException($"{family}/{name}: checksum mismatch");
        }
        return Stopwatch.GetElapsedTime(start).TotalMilliseconds / repeats;
    }

    static int Main(string[] args)
    {
        if (args.Length != 1) throw new ArgumentException("Expected a benchmark plan JSON path");
        using var plan = JsonDocument.Parse(File.ReadAllText(args[0]));
        bool quick = plan.RootElement.GetProperty("quick").GetBoolean();
        var workloads = new List<object>();
        foreach (var job in plan.RootElement.GetProperty("jobs").EnumerateArray())
        {
            string family = job.GetProperty("family").GetString()!;
            string name = job.GetProperty("name").GetString()!;
            long size = job.GetProperty("size").GetInt64();
            int checks = 0;
            foreach (var check in job.GetProperty("checks").EnumerateArray())
            {
                ulong seed = ulong.Parse(check.GetProperty("seed").GetString()!, CultureInfo.InvariantCulture);
                ulong expected = ulong.Parse(check.GetProperty("checksum").GetString()!, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                if (Kernels.Run(family, name, check.GetProperty("size").GetInt64(), seed) != expected)
                    throw new InvalidOperationException($"{family}/{name}: reference mismatch");
                ++checks;
            }
            var first = job.GetProperty("samples")[0];
            ulong warmSeed = ulong.Parse(first.GetProperty("seed").GetString()!, CultureInfo.InvariantCulture);
            ulong warmExpected = ulong.Parse(first.GetProperty("checksum").GetString()!, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
            for (int warm = 0; warm < 3; ++warm) Measure(family, name, size, warmSeed, warmExpected, 1);
            double calibration = Measure(family, name, size, warmSeed, warmExpected, 1);
            int repeats = quick ? 1 : (int)Math.Min(10000, Math.Ceiling(20 / Math.Max(0.001, calibration)));
            var raw = new List<object>();
            foreach (var sample in job.GetProperty("samples").EnumerateArray())
            {
                ulong seed = ulong.Parse(sample.GetProperty("seed").GetString()!, CultureInfo.InvariantCulture);
                string checksum = sample.GetProperty("checksum").GetString()!;
                ulong expected = ulong.Parse(checksum, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                int[] collections = [GC.CollectionCount(0), GC.CollectionCount(1), GC.CollectionCount(2)];
                long allocated = GC.GetTotalAllocatedBytes(true);
                double wall = Measure(family, name, size, seed, expected, repeats);
                raw.Add(new {
                    wall_ms = wall, repeats, checksum,
                    allocated_bytes = GC.GetTotalAllocatedBytes(true) - allocated,
                    gc_collections = new[] { GC.CollectionCount(0) - collections[0], GC.CollectionCount(1) - collections[1], GC.CollectionCount(2) - collections[2] },
                });
            }
            long reclaimStart = Stopwatch.GetTimestamp();
            GC.Collect();
            GC.WaitForPendingFinalizers();
            workloads.Add(new { family, name, checks, raw, post_measurement_gc_ms = Stopwatch.GetElapsedTime(reclaimStart).TotalMilliseconds });
        }
        Console.WriteLine(JsonSerializer.Serialize(new {
            environment = new {
                runtime = RuntimeInformation.FrameworkDescription,
                architecture = RuntimeInformation.ProcessArchitecture.ToString(),
                clock = "Stopwatch monotonic wall time",
                tiered_compilation = Environment.GetEnvironmentVariable("DOTNET_TieredCompilation"),
                gc_server = System.Runtime.GCSettings.IsServerGC,
                array_storage = "NativeMemory with Span traversal; matching explicit allocation/free",
            }, workloads,
        }));
        return 0;
    }
}

static unsafe class Kernels
{
    const ulong Multiplier = 6364136223846793005, Increment = 1442695040888963407;
    struct Node { public ulong Value; public Node* Next; }
    readonly record struct State<T>(T Value, long Remaining);

    static ulong Mix(ulong value, ulong salt) => unchecked((value ^ (value >> 13)) * Multiplier + salt);
    static ulong Step(ulong value, ulong salt) => Mix(value, unchecked(salt + Increment));

    [MethodImpl(MethodImplOptions.NoInlining)]
    public static ulong Run(string family, string name, long count, ulong seed)
    {
        if (count < 0 || count > 100000000) throw new ArgumentOutOfRangeException(nameof(count));
        if (family == "cpp" && name == "integer_mix")
        {
            for (long index = 0; index < count; ++index) seed = Mix(seed, Increment);
            return seed;
        }
        if (family == "cpp" && name == "mandelbrot") return Mandelbrot(count, unchecked((long)seed));
        if (family == "cpp" && name.StartsWith("task_", StringComparison.Ordinal)) return Tasks(name, count, seed);
        if (family == "cpp" && name is "utf16_scan" or "utf16_compare" or "utf16_validate" or "utf8_roundtrip" or "format_parse" or "math_intrinsics")
            return TextAndMath(name, count, seed);
        if (family == "computations") return Computation(name, count, seed);
        switch (name)
        {
            case "while_mix": case "for_mix": case "tail_mix": case "tail_if_mix": case "tail_builtin_mix":
                for (long remaining = count; remaining > 0; --remaining) seed = Step(seed, (ulong)remaining);
                return seed;
            case "match_dispatch":
                ulong total = seed;
                for (long index = 0; index < count; ++index)
                {
                    ulong factor = (unchecked(seed + (ulong)index) % 16) switch {
                        0 => 17, 1 => 3, 2 => 29, 3 => 7, 4 => 61, 5 => 11, 6 => 83, 7 => 5,
                        8 => 47, 9 => 19, 10 => 101, 11 => 31, 12 => 53, 13 => 23, 14 => 97, _ => 13,
                    };
                    total = unchecked(total + factor * ((ulong)index + 1));
                }
                return total;
            case "array_sum": return Array(count, seed, false, false);
            case "array_copy": return Array(count, seed, true, false);
            case "list_sum": return List(count, seed);
            case "closure_capture": return ClosureCapture(count, seed);
            case "record_pipeline":
                var record = new State<ulong>(seed, count);
                while (record.Remaining > 0) record = new State<ulong>(Step(record.Value, (ulong)record.Remaining), record.Remaining - 1);
                return record.Value;
            case "integer128_mix":
                UInt128 wide = ((UInt128)seed << 64) | Increment;
                for (long remaining = count; remaining > 0; --remaining)
                    wide = unchecked((wide ^ (wide >> 43)) * Multiplier + (ulong)remaining);
                return unchecked((ulong)(wide ^ (wide >> 64)));
            case "float32_mix":
                float single = (seed & 65535) / 16.0f + 1.0f;
                for (long index = 0; index < count; ++index) single = single * 1.000001f + (index & 7) / 16.0f;
                return single >= 18446744073709551616.0f ? ulong.MaxValue : (ulong)single;
            case "float64_mix":
                double real = (seed & 65535) / 16.0 + 1.0;
                for (long index = 0; index < count; ++index) real = real * 1.0000001 + (index & 7) / 16.0;
                return real >= 18446744073709551616.0 ? ulong.MaxValue : (ulong)real;
            default: throw new ArgumentException($"Unknown workload: {family}/{name}");
        }
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    static ulong ClosureCapture(long count, ulong seed)
    {
        Func<ulong, ulong> transform = (seed & 1) == 0 ? value => Step(value, seed) : value => Step(value, seed ^ 71);
        ulong state = seed;
        for (long index = 0; index < count; ++index) state = transform(state);
        return state;
    }

    static ulong Array(long count, ulong seed, bool copy, bool computation)
    {
        ulong* data = (ulong*)NativeMemory.Alloc((nuint)Math.Max(1, count * 8));
        if (data == null) throw new OutOfMemoryException();
        ulong* duplicate = null;
        try
        {
            var values = new Span<ulong>(data, (int)count);
            for (int index = 0; index < values.Length; ++index)
                values[index] = computation ? Mix((ulong)index, seed) : unchecked(((ulong)index ^ seed) * Multiplier + Increment);
            if (copy)
            {
                duplicate = (ulong*)NativeMemory.Alloc((nuint)Math.Max(1, count * 8));
                if (duplicate == null) throw new OutOfMemoryException();
                values.CopyTo(new Span<ulong>(duplicate, (int)count));
            }
            ulong total = 0;
            foreach (ulong value in values)
                total = unchecked(total + (computation ? (value ^ (value >> 17)) * (seed | 1) : value));
            if (copy) foreach (ulong value in new Span<ulong>(duplicate, (int)count)) total = unchecked(total + value);
            return total;
        }
        finally { NativeMemory.Free(duplicate); NativeMemory.Free(data); }
    }

    static ulong List(long count, ulong seed)
    {
        Node* head = null;
        Node** tail = &head;
        try
        {
            for (long index = 0; index < count; ++index)
            {
                Node* node = (Node*)NativeMemory.Alloc((nuint)sizeof(Node));
                if (node == null) throw new OutOfMemoryException();
                node->Value = unchecked(((ulong)index ^ seed) * Multiplier + Increment);
                node->Next = null;
                *tail = node;
                tail = &node->Next;
            }
            ulong total = 0;
            for (Node* node = head; node != null; node = node->Next) total = unchecked(total + node->Value);
            return total;
        }
        finally
        {
            while (head != null) { Node* next = head->Next; NativeMemory.Free(head); head = next; }
        }
    }

    static ulong Mandelbrot(long size, long seed)
    {
        if (size > 4096) throw new ArgumentOutOfRangeException(nameof(size));
        if (size == 0) return 0;
        double dx = 3.0 / size, dy = 2.0 / size, offset = seed * 0.000001;
        ulong total = 0;
        for (long index = 0; index < size * size; ++index)
        {
            double cr = index % size * dx - 2.0 + offset, ci = index / size * dy - 1.0;
            double real = 0, imaginary = 0;
            int count = 0;
            while (count < 256 && real * real + imaginary * imaginary <= 4.0)
            {
                double next = real * real - imaginary * imaginary + cr;
                imaginary = 2.0 * real * imaginary + ci;
                real = next;
                ++count;
            }
            total += (ulong)count;
        }
        return total;
    }

    static ulong Tasks(string name, long count, ulong seed)
    {
        if (name == "task_parallel") return ParallelTasks(count, seed);
        if (name == "task_sequence")
        {
            for (long index = 0; index < count; ++index)
            {
                ulong captured = seed;
                Func<ulong> task = () => captured;
                seed = Mix(task(), Increment);
            }
            return seed;
        }
        var results = new ulong[16];
        for (int index = 0; index < results.Length; ++index) results[index] = Run("cpp", "integer_mix", count, seed ^ (ulong)index);
        ulong total = 0;
        foreach (ulong value in results) total = unchecked(total + value);
        return total;
    }

    static ulong ParallelTasks(long count, ulong seed)
    {
        var results = new ulong[16];
        Parallel.For(0, results.Length, new ParallelOptions { MaxDegreeOfParallelism = Math.Min(32, Environment.ProcessorCount) },
            index => results[index] = Run("cpp", "integer_mix", count, seed ^ (ulong)index));
        ulong total = 0;
        foreach (ulong value in results) total = unchecked(total + value);
        return total;
    }

    static string MakeText(long count, ulong seed)
    {
        if (count == 0) return "";
        string text = (seed & 1) == 0 ? "Az09-_ \n" : "A\0\u03A9\uD83D\uDE00\u4E2Dz\n";
        while (text.Length < count) { string copy = new(text.AsSpan()); text = copy + text; }
        return text;
    }

    static ulong TextSum(string text)
    {
        ulong total = 0;
        foreach (char unit in text) total += unit;
        return total;
    }

    static bool WellFormed(string text)
    {
        for (int index = 0; index < text.Length; ++index)
        {
            if (char.IsHighSurrogate(text[index]))
            {
                if (index + 1 == text.Length || !char.IsLowSurrogate(text[index + 1])) return false;
                ++index;
            }
            else if (char.IsLowSurrogate(text[index])) return false;
        }
        return true;
    }

    static ulong TextAndMath(string name, long count, ulong seed)
    {
        if (name == "format_parse")
        {
            for (long index = 0; index < count; ++index)
            {
                seed = Step(seed, (ulong)index);
                string formatted = seed.ToString(CultureInfo.InvariantCulture);
                seed = ulong.Parse(formatted, NumberStyles.None, CultureInfo.InvariantCulture) ^ (ulong)formatted.Length;
            }
            return seed;
        }
        if (name == "math_intrinsics")
        {
            double state = ((seed & 65535) + 1) / 16.0;
            for (long index = 0; index < count; ++index)
            {
                state = Math.Sqrt(Math.Abs(state) + (index & 255));
                state = Math.Floor(state * 16.0) / 16.0 + Math.Ceiling(state) / 1024.0;
            }
            return (ulong)(state * 1048576.0);
        }
        string text = MakeText(count, seed);
        if (name == "utf16_scan") return TextSum(text);
        if (name == "utf16_compare")
        {
            string copy = new(text.AsSpan());
            string left = copy + ((seed & 2) == 0 ? "a" : "b"), right = text + ((seed & 4) == 0 ? "a" : "b");
            return left == right ? 1UL : string.CompareOrdinal(left, right) < 0 ? 2UL : 4UL;
        }
        if (name == "utf16_validate")
        {
            text += (seed & 8) == 0 ? "" : "\uD800";
            string repaired = string.Create(text.Length, text, static (units, source) => {
                source.AsSpan().CopyTo(units);
                for (int index = 0; index < units.Length; ++index)
                {
                    if (char.IsHighSurrogate(units[index]))
                    {
                        if (index + 1 < units.Length && char.IsLowSurrogate(units[index + 1])) ++index;
                        else units[index] = '\uFFFD';
                    }
                    else if (char.IsLowSurrogate(units[index])) units[index] = '\uFFFD';
                }
            });
            return TextSum(repaired) + (WellFormed(text) ? 1UL : 0UL);
        }
        var encoding = new UTF8Encoding(false, true);
        byte[] bytes = encoding.GetBytes(text);
        string restored = encoding.GetString(bytes);
        if (restored != text) throw new InvalidOperationException("UTF roundtrip mismatch");
        return TextSum(restored) + (ulong)bytes.Length;
    }

    static ulong Computation(string name, long count, ulong seed)
    {
        if (name is "array_for" or "array_bind") return Array(count, seed, false, true);
        Span<ulong> values = stackalloc ulong[256];
        if (name == "owned_capture")
            for (int index = 0; index < values.Length; ++index) values[index] = Mix((ulong)index, seed);
        ulong state = seed;
        for (long remaining = count; remaining > 0; --remaining)
        {
            ulong salt = unchecked(Increment + (ulong)remaining);
            switch (name)
            {
                case "bind": state = Mix(state, salt); break;
                case "checked": case "std_result":
                    ulong first = state ^ salt;
                    state = (state & 7) == 0 ? first : (first & 3) == 0 ? Mix(first, salt) : Mix(first, salt) ^ salt;
                    break;
                case "delayed": state = unchecked(Mix(state, salt) + Mix(state ^ 71, salt) + Mix(state ^ 113, salt)); break;
                case "std_option":
                    ulong? option = (state & 7) == 0 ? null : state ^ salt;
                    state = option.HasValue ? Mix(option.Value, salt) : state ^ salt;
                    break;
                case "std_option_owned": state = Mix(state, salt) ^ ((state & 7) == 0 ? 0UL : 12UL); break;
                case "owned_capture": state = Mix(values[(int)(state & 255)], state); break;
                default: throw new ArgumentException($"Unknown computation: {name}");
            }
        }
        return state;
    }
}