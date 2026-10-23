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
ulong Ext(ulong v) => new Hybrid<ulong>(v, isExternal: true).Value;
foreach (var (name, open) in new (string, Func<ILinks<ulong>>)[]
{
    ("united", () => new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory(), UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, external, IndexTreeType.Default)),
    ("united-avl", () => new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory(), UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, external, IndexTreeType.SizedAndThreadedAVLBalancedTree)),
    ("split", () => new SplitMemoryLinks<ulong>(new HeapResizableDirectMemory(), new HeapResizableDirectMemory(), SplitMemoryLinks<ulong>.DefaultLinksSizeStep, external)),
})
{
    var links = open();
    var any = links.Constants.Any;
    var model = new Dictionary<ulong, (ulong S, ulong T)>();
    var random = new Random(1);
    string? error = null;
    ulong Value()
    {
        if (mixInternal && model.Count > 0 && random.Next(2) == 0) { return model.Keys.ElementAt(random.Next(model.Count)); }
        return Ext((ulong)random.Next(1, 30));
    }
    List<ulong> Each(ulong s, ulong t)
    {
        var r = new List<ulong>();
        links.Each(l => { r.Add(l![0]); return links.Constants.Continue; }, any, s, t);
        r.Sort();
        return r;
    }
    int step;
    for (step = 0; step < 5000 && error is null; step++)
    {
        var op = random.Next(3);
        string what;
        if (op == 0 || model.Count < 3)
        {
            var (s, t) = (Value(), Value());
            // Doublets keeps every (source, target) pair once, so a duplicate is skipped.
            if (model.ContainsValue((s, t))) { continue; }
            var id = links.CreateAndUpdate(s, t);
            model[id] = (s, t);
            what = $"create {id}=({s},{t})";
        }
        else
        {
            var id = model.Keys.ElementAt(random.Next(model.Count));
            if (op == 1)
            {
                var (s, t) = (Value(), Value());
                if (model.ContainsValue((s, t))) { continue; }
                links.Update(id, s, t);
                model[id] = (s, t);
                what = $"update {id}=({s},{t})";
            }
            else
            {
                // A link that others refer to cannot be deleted; skip those.
                if (model.Values.Any(v => v.S == id || v.T == id)) { continue; }
                links.Delete(id, handler: null);
                model.Remove(id);
                what = $"delete {id}";
            }
        }
        foreach (var (id, (s, t)) in model)
        {
            var expectedPair = model.Where(e => e.Value == (s, t)).Select(e => e.Key).Order().ToList();
            var expectedOut = model.Where(e => e.Value.S == s).Select(e => e.Key).Order().ToList();
            var expectedIn = model.Where(e => e.Value.T == t).Select(e => e.Key).Order().ToList();
            if (!Each(s, t).SequenceEqual(expectedPair)) { error = $"concrete ({s},{t}): got [{string.Join(",", Each(s, t))}], expected [{string.Join(",", expectedPair)}]"; }
            else if (!Each(s, any).SequenceEqual(expectedOut)) { error = $"outgoing {s}: got [{string.Join(",", Each(s, any))}], expected [{string.Join(",", expectedOut)}]"; }
            else if (!Each(any, t).SequenceEqual(expectedIn)) { error = $"incoming {t}: got [{string.Join(",", Each(any, t))}], expected [{string.Join(",", expectedIn)}]"; }
            else if (links.Count() != (ulong)model.Count) { error = $"count {links.Count()} != {model.Count}"; }
            if (error is not null) { error = $"after `{what}`: {error}"; break; }
        }
    }
    Console.WriteLine($"{name} ({(mixInternal ? "internal and external" : "external only")}): {error ?? "OK"} (step {step})");
}
