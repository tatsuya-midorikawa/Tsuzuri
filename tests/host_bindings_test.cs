// Checks the C# bindings of tests/fixtures/bindings_native (tests/host_bindings.mjs): Native is the
// library of --allocator counting, NativeTrap the library of --trap-mode return.
// usage: dotnet bindings.dll LIBRARY TRAP_LIBRARY
using System;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using Tsuzuri.Bindings;

string library = args[0];
string trapLibrary = args[1];
NativeLibrary.SetDllImportResolver(typeof(Native).Assembly, (name, _, _) => name switch
{
    Native.LibraryName => NativeLibrary.Load(library),
    NativeTrap.LibraryName => NativeLibrary.Load(trapLibrary),
    _ => IntPtr.Zero,
});

static void Check(bool condition, string what)
{
    if (!condition)
    {
        throw new InvalidOperationException($"check failed: {what}");
    }
}

static T Throws<T>(Action call, string what) where T : Exception
{
    try
    {
        call();
    }
    catch (T error)
    {
        return error;
    }
    throw new InvalidOperationException($"expected {typeof(T).Name}: {what}");
}

Check(Native.add(40, 2) == 42, "add");
Check(Native.add_u64(ulong.MaxValue, 0) == ulong.MaxValue, "add_u64");
Check(Native.widen(-128, 65535, 4294967295) == -128L + 65535 + 4294967295, "widen");
Check(Native.narrow(300) == 300 - 256 && Native.narrow(-129) == 127, "narrow");
Check(!Native.negate(true) && Native.negate(false), "negate");
Check(Native.twice32(0.1f) == 0.1f * 2, "twice32");
Check(Native.sum_float([0.5, 0.25]) == 0.75 && Native.sum_float([]) == 0.0, "sum_float");
Check(Native.checksum([1, 2, 255]) == 258, "checksum");
using (var made = Native.make_bytes(4))
{
    Check(made.Span.SequenceEqual(new byte[] { 0, 1, 2, 3 }), "make_bytes");
}
using (var empty = Native.make_bytes(0))
{
    Check(empty.Length == 0 && empty.Span.IsEmpty, "make_bytes 0");
}
long[] values = [1, -2, long.MaxValue];
using (var copy = Native.copy_values(values))
{
    Check(copy.ToArray().SequenceEqual(values), "copy_values");
    copy.Dispose();
    Throws<ObjectDisposedException>(() => _ = copy.Span.Length, "a disposed buffer");
}
string text = "a\ud800b\U0001F600";
using (var copied = Native.copy_text(text))
{
    Check(copied.ToString() == text && copied.Length == text.Length, "copy_text");
}
using (var utf8 = Native.copy_utf8("\u00e9\U0001F600"u8))
{
    Check(utf8.ToString() == "\u00e9\U0001F600", "copy_utf8");
}
Throws<ArgumentException>(() => Native.copy_utf8([0xED, 0xA0, 0x80]).Dispose(), "invalid UTF-8");
var sample = new Native.tz_record_4Main_6Sample { amount = 1.5 };
sample.flags.tiny = -1;
sample.flags.wide = 65535;
sample.flags.flag = true;
var updated = Native.update(sample);
Check(updated.flags.tiny == -1 && updated.flags.wide == 65535 && !updated.flags.flag && updated.amount == 2.5, "update");
Check(Native.area(new Native.tz_record_8Geometry_5Point { x = 2, y = 3.5 }) == 7.0, "area");
var window = new Native.tz_record_4Main_6Window { scale = 2.0, mark = 7 };
window.values[0] = 1;
window.values[1] = 2;
window.values[2] = 3;
Check(Native.window_total(window) == 19.0, "window_total");
var made_window = Native.make_window(5);
Check(made_window.values[0] == 5 && made_window.values[1] == 10 && made_window.values[2] == -5 && made_window.scale == 0.5 && made_window.mark == 255, "make_window");
Check(Native.counters(10) == 30, "counters");
var opaque = new Native.tz_handle_4Main_7Counter(0x1234);
Check(Native.pass_through(opaque) == opaque, "pass_through");
unsafe
{
    // A borrowed handle reaches the host as the pointer itself: here, an int64 counter cell.
    long cell = 41;
    Check(Native.peek(new Native.tz_handle_4Main_7Counter((nint)(&cell))) == 41, "peek");
}
using (var scaled = Native.scaled([1.0, 2.0], 3.0))
{
    Check(scaled.Span.SequenceEqual(new[] { 3.0, 6.0 }), "scaled");
}
Native.check(5);

// Temporaries: the finalizer of an unreachable result must not free it while ToArray or ToString
// copies it. Another thread collects all the time; the host fills freed memory (MallocScribble).
var stop = 0;
// A background thread that is always stopped, so a failed check cannot keep the process alive.
var collector = new Thread(() =>
{
    while (Volatile.Read(ref stop) == 0)
    {
        GC.Collect();
        GC.WaitForPendingFinalizers();
    }
}) { IsBackground = true };
collector.Start();
try
{
    string large = new string('x', 1 << 16) + "\ud800";
    for (int round = 0; round < 1000; round++)
    {
        byte[] bytes = Native.make_bytes(1 << 16).ToArray();
        bool intact = bytes.Length == 1 << 16;
        for (int index = 0; intact && index < bytes.Length; index++)
        {
            intact = bytes[index] == (byte)index;
        }
        Check(intact, $"make_bytes(...).ToArray() in round {round}");
        Check(Native.copy_text(large).ToString() == large, $"copy_text(...).ToString() in round {round}");
        Check(Native.copy_utf8("\u00e9t\u00e9"u8).ToString() == "\u00e9t\u00e9", $"copy_utf8(...).ToString() in round {round}");
    }
}
finally
{
    Volatile.Write(ref stop, 1);
    collector.Join();
}
GC.Collect();
GC.WaitForPendingFinalizers();

unsafe
{
    ulong* stats = stackalloc ulong[4];
    Stats.tsuzuri_alloc_stats(stats);
    Check(stats[2] == 0 && stats[0] == stats[1], $"live bytes {stats[2]}");
}

// --trap-mode return: a trap becomes an exception, and the next call runs normally.
var trap = Throws<NativeTrap.TsuzuriTrapException>(() => NativeTrap.divide(1, 0), "divide by zero");
Check(trap.Kind == 1 && trap.KindName == "integer division by zero" && trap.Message.StartsWith("trap: integer division by zero (site "), "trap");
Check(NativeTrap.divide(-7, 2) == -3, "after a trap");
Check(Throws<NativeTrap.TsuzuriTrapException>(() => NativeTrap.check(-1), "assert").KindName == "assertion failed", "assert");
using (var after = NativeTrap.copy_text("after"))
{
    Check(after.ToString() == "after", "copy_text after a trap");
}
Check(NativeTrap.add(40, 2) == 42, "trap add");
Console.WriteLine($"csharp bindings passed (trap site {trap.Site})");

internal static unsafe partial class Stats
{
    [LibraryImport("native")]
    internal static partial void tsuzuri_alloc_stats(ulong* stats);
}
