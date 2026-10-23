# Experiments

Checks made while preparing the benchmarks of
[issue #1](https://github.com/linksplatform/Comparisons.RedisVSDoublets/issues/1).
They are not part of the CI. Outputs are in [`logs`](logs).

| Directory | What it checks |
| --- | --- |
| [`csharp-external-references`](csharp-external-references) | random creates, updates and deletes of links with external references (raw numbers) in the C# stores, compared with a plain list after every step |
| [`rust-external-references`](rust-external-references) | the same check for the Rust stores of doublets 0.5.0 |
| [`csharp-usage-index-trees`](csharp-usage-index-trees) | the C# united store's index tree types with the set links of the usage benchmarks (all members share one source) |

External references
([`logs/csharp-external-references.log`](logs/csharp-external-references.log)):
with only external references every store is correct; when links mix
internal and external references, the `SplitMemoryLinks` of
Platform.Data.Doublets 0.18.1 loses links from its concrete index: after
`create 3=(2,18446744073709551595)` the query `(2, 18446744073709551595)`
finds nothing instead of link 3. The C# usage benchmarks keep their queue
nodes as `(next node, raw value)`, so they use the united store only.
The Rust `split::Store` of doublets 0.5.0 has the same defect
([`logs/rust-external-references.log`](logs/rust-external-references.log)):
after `update 1=(2, 1)` with an external target the incoming query of
link 1 misses it, so the Rust usage benchmarks also use the united store only.

```bash
cd experiments/csharp-external-references
dotnet run -c Release           # external references only
dotnet run -c Release -- mixed  # internal and external references
cd ../rust-external-references
cargo run --release
cargo run --release -- mixed
```

Index trees ([`logs/csharp-usage-index-trees.log`](logs/csharp-usage-index-trees.log)):
the SQLite comparison switched its C# united stores to AVL trees, because the
default size balanced trees degenerate with its blog post links. The set links
here do not grow with the number of members with any tree type (the times are
noisy, the host was loaded), so the benchmarks keep the default trees.

```bash
cd experiments/csharp-usage-index-trees
dotnet run -c Release -- 200000
```
