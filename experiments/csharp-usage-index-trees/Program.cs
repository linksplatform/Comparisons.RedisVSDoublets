// Times the set operations of the usage benchmarks on Platform.Data.Doublets 0.18.1 UnitedMemoryLinks with
// each index tree type: all set members are links (set, raw member) that share one source, the set point.
// Usage: dotnet run -c Release -- [members]   (default 20,000)
// Prints the mean time of SADD (create the link), SISMEMBER (search the link) and SREM (delete it) for the
// first and the last tenth of the members.
using System.Diagnostics;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

var n = args.Length > 0 ? ulong.Parse(args[0]) : 20_000UL;
var tenth = n / 10;
var constants = new LinksConstants<ulong>(enableExternalReferencesSupport: true);
ulong Raw(ulong v) => new Hybrid<ulong>(v, isExternal: true).Value;
// The first round only warms up the JIT.
foreach (var (round, type) in Enum.GetValues<IndexTreeType>().Select(t => (0, t)).Concat(Enum.GetValues<IndexTreeType>().Select(t => (1, t))))
{
    var links = new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory(), UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, constants, type);
    var set = links.CreatePoint();
    var add = new double[2];
    var contains = new double[2];
    var remove = new double[2];
    var ids = new ulong[n];
    double Time(Action action) { var start = Stopwatch.GetTimestamp(); action(); return Stopwatch.GetElapsedTime(start).TotalNanoseconds; }
    int? Bucket(ulong i) => i < tenth ? 0 : i >= n - tenth ? 1 : null;
    for (ulong i = 0; i < n; i++)
    {
        var t = Time(() => ids[i] = links.GetOrCreate(set, Raw(i + 1)));
        if (Bucket(i) is { } b) { add[b] += t; }
    }
    for (ulong i = 0; i < n; i++)
    {
        var t = Time(() => { if (links.SearchOrDefault(set, Raw(i + 1)) != ids[i]) { throw new Exception("not found"); } });
        if (Bucket(i) is { } b) { contains[b] += t; }
    }
    for (ulong i = n; i-- > 0;)
    {
        var t = Time(() => links.Delete(ids[i], handler: null));
        if (Bucket(i) is { } b) { remove[b] += t; }
    }
    string F(double[] a) => $"{a[0] / tenth / 1000,6:F1} µs -> {a[1] / tenth / 1000,6:F1} µs";
    if (round == 1) { Console.WriteLine($"{type,-34} add {F(add)}   contains {F(contains)}   remove {F(remove)}"); }
}
