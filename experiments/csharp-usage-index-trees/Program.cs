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
// The first round only warms up the JIT.
foreach (var round in new[] { 0, 1 })
{
    foreach (var type in Enum.GetValues<IndexTreeType>())
    {
        var times = new SetTimes(n, type).Measure();
        if (round == 1) { Console.WriteLine(times); }
    }
}

/// <summary>Times adding, searching and removing <c>n</c> members of one set in a new store.</summary>
internal sealed class SetTimes(ulong n, IndexTreeType type)
{
    private static readonly LinksConstants<ulong> Constants = new(enableExternalReferencesSupport: true);
    private readonly UnitedMemoryLinks<ulong> _links = new(new HeapResizableDirectMemory(), UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, Constants, type);
    private readonly ulong[] _ids = new ulong[n];
    private readonly ulong _tenth = n / 10;

    public string Measure()
    {
        var set = _links.CreatePoint();
        var add = Time(Ascending(), i => _ids[i] = _links.GetOrCreate(set, Raw(i + 1)));
        var contains = Time(Ascending(), i => { if (_links.SearchOrDefault(set, Raw(i + 1)) != _ids[i]) { throw new Exception("not found"); } });
        var remove = Time(Descending(), i => _links.Delete(_ids[i], handler: null));
        return $"{type,-34} add {Format(add)}   contains {Format(contains)}   remove {Format(remove)}";
    }

    private static ulong Raw(ulong v) => new Hybrid<ulong>(v, isExternal: true).Value;

    private IEnumerable<ulong> Ascending()
    {
        for (ulong i = 0; i < n; i++) { yield return i; }
    }

    private IEnumerable<ulong> Descending()
    {
        for (var i = n; i-- > 0;) { yield return i; }
    }

    /// <summary>Sums the times of <paramref name="action"/> for the first and the last tenth of the members.</summary>
    private double[] Time(IEnumerable<ulong> members, Action<ulong> action)
    {
        var sums = new double[2];
        foreach (var i in members)
        {
            var start = Stopwatch.GetTimestamp();
            action(i);
            var elapsed = Stopwatch.GetElapsedTime(start).TotalNanoseconds;
            if (Bucket(i) is { } b) { sums[b] += elapsed; }
        }
        return sums;
    }

    private int? Bucket(ulong i) => i < _tenth ? 0 : i >= n - _tenth ? 1 : null;

    private string Format(double[] sums) => $"{sums[0] / _tenth / 1000,6:F1} µs -> {sums[1] / _tenth / 1000,6:F1} µs";
}
