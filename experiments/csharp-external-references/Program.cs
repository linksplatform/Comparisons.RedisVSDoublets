// Differential check of Platform.Data.Doublets 0.18.1 stores with external references (raw numbers):
// random creates, updates and deletes of links that mix internal and external references,
// compared after every step with a plain list. Prints the first mismatch of each store.
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Data.Doublets.Memory.Split.Generic;
using Platform.Memory;

var mixInternal = args.Length > 0 && args[0] == "mixed";
var external = new LinksConstants<ulong>(true);
foreach (var (name, open) in new (string, Func<ILinks<ulong>>)[]
{
    ("united", () => new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory(), UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, external, IndexTreeType.Default)),
    ("united-avl", () => new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory(), UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, external, IndexTreeType.SizedAndThreadedAVLBalancedTree)),
    ("split", () => new SplitMemoryLinks<ulong>(new HeapResizableDirectMemory(), new HeapResizableDirectMemory(), SplitMemoryLinks<ulong>.DefaultLinksSizeStep, external)),
})
{
    var (step, error) = new Check(open(), mixInternal).Run(5000);
    Console.WriteLine($"{name} ({(mixInternal ? "internal and external" : "external only")}): {error ?? "OK"} (step {step})");
}

/// <summary>Applies the same random changes to a store and to a plain list and compares their queries.</summary>
internal sealed class Check(ILinks<ulong> links, bool mixInternal)
{
    private readonly ulong _any = links.Constants.Any;
    private readonly Dictionary<ulong, (ulong S, ulong T)> _model = new();
    private readonly Random _random = new(1);

    /// <summary>Returns the number of steps done and the first mismatch, if any.</summary>
    public (int Step, string? Error) Run(int steps)
    {
        for (var step = 0; step < steps; step++)
        {
            if (Change() is { } what && Compare() is { } error)
            {
                return (step + 1, $"after `{what}`: {error}");
            }
        }
        return (steps, null);
    }

    /// <summary>Makes a random change and describes it, or returns null if the change was skipped.</summary>
    private string? Change()
    {
        var op = _random.Next(3);
        if (op == 0 || _model.Count < 3)
        {
            return Create();
        }
        var id = _model.Keys.ElementAt(_random.Next(_model.Count));
        return op == 1 ? Update(id) : Delete(id);
    }

    private string? Create()
    {
        var (s, t) = (Value(), Value());
        // Doublets keeps every (source, target) pair once, so a duplicate is skipped.
        if (_model.ContainsValue((s, t))) { return null; }
        var id = links.CreateAndUpdate(s, t);
        _model[id] = (s, t);
        return $"create {id}=({s},{t})";
    }

    private string? Update(ulong id)
    {
        var (s, t) = (Value(), Value());
        if (_model.ContainsValue((s, t))) { return null; }
        links.Update(id, s, t);
        _model[id] = (s, t);
        return $"update {id}=({s},{t})";
    }

    private string? Delete(ulong id)
    {
        // A link that others refer to cannot be deleted; skip those.
        if (_model.Values.Any(v => v.S == id || v.T == id)) { return null; }
        links.Delete(id, handler: null);
        _model.Remove(id);
        return $"delete {id}";
    }

    private ulong Value()
    {
        if (mixInternal && _model.Count > 0 && _random.Next(2) == 0) { return _model.Keys.ElementAt(_random.Next(_model.Count)); }
        return new Hybrid<ulong>((ulong)_random.Next(1, 30), isExternal: true).Value;
    }

    /// <summary>Returns the first query of the store that differs from the plain list, or null.</summary>
    private string? Compare()
    {
        foreach (var (s, t) in _model.Values)
        {
            var error = Mismatch($"concrete ({s},{t})", s, t, v => v == (s, t))
                ?? Mismatch($"outgoing {s}", s, _any, v => v.S == s)
                ?? Mismatch($"incoming {t}", _any, t, v => v.T == t)
                ?? (links.Count() != (ulong)_model.Count ? $"count {links.Count()} != {_model.Count}" : null);
            if (error is not null) { return error; }
        }
        return null;
    }

    private string? Mismatch(string query, ulong s, ulong t, Func<(ulong S, ulong T), bool> matches)
    {
        var expected = _model.Where(e => matches(e.Value)).Select(e => e.Key).Order().ToList();
        var got = Each(s, t);
        return got.SequenceEqual(expected) ? null : $"{query}: got [{string.Join(",", got)}], expected [{string.Join(",", expected)}]";
    }

    private List<ulong> Each(ulong s, ulong t)
    {
        var r = new List<ulong>();
        links.Each(l => { r.Add(l![0]); return links.Constants.Continue; }, _any, s, t);
        r.Sort();
        return r;
    }
}
