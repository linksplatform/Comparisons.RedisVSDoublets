## Summary

`SplitMemoryLinks<ulong>` of Platform.Data.Doublets 0.18.1, with external
references enabled, cannot find a link that has an internal source and an
external target. `UnitedMemoryLinks<ulong>` with the same constants finds it.

## Minimal reproduction (Platform.Data.Doublets 0.18.1, .NET 10, Linux x86_64)

```csharp
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Data.Doublets.Memory.Split.Generic;
using Platform.Memory;

var constants = new LinksConstants<ulong>(true);
var x = new Hybrid<ulong>(20, isExternal: true).Value;
foreach (var (name, links) in new (string, ILinks<ulong>)[]
{
    ("united", new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory(), UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, constants, IndexTreeType.Default)),
    ("split", new SplitMemoryLinks<ulong>(new HeapResizableDirectMemory(), new HeapResizableDirectMemory(), SplitMemoryLinks<ulong>.DefaultLinksSizeStep, constants)),
})
{
    var a = links.CreatePoint();          // 1 = (1, 1)
    var b = links.CreateAndUpdate(a, x);  // 2 = (1, x): internal source, external target
    var found = new List<ulong>();
    links.Each(l => { found.Add(l![0]); return links.Constants.Continue; }, links.Constants.Any, a, x);
    Console.WriteLine($"{name}: created {b} = ({a}, {x}); Each(any, {a}, {x}) -> [{string.Join(",", found)}], SearchOrDefault -> {links.SearchOrDefault(a, x)}");
}
```

Output:

```
united: created 2 = (1, 18446744073709551596); Each(any, 1, 18446744073709551596) -> [2], SearchOrDefault -> 2
split: created 2 = (1, 18446744073709551596); Each(any, 1, 18446744073709551596) -> [], SearchOrDefault -> 0
```

Expected: `split` finds link 2, as `united` does.

## Notes

- With external references only (both source and target external) every
  store is correct. The random differential check, its log and the commands are in
  [Comparisons.RedisVSDoublets/experiments/csharp-external-references](https://github.com/linksplatform/Comparisons.RedisVSDoublets/tree/issue-13-f67d791f1225/experiments/csharp-external-references).
- The Rust `split::Store` of doublets 0.5.0 had a similar defect with external
  targets; doublets 0.8.0 passes the same check.

## Workaround downstream

The C# usage benchmarks of `Comparisons.RedisVSDoublets` keep their queue
nodes as `(next node, raw value)` and therefore use `UnitedMemoryLinks` only.
