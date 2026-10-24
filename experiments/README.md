# Experiments

Checks made while preparing the benchmarks of
[issue #1](https://github.com/linksplatform/Comparisons.RedisVSDoublets/issues/1).
They are not part of the CI. Outputs are in [`logs`](logs).

| Experiment | What it checks |
| --- | --- |
| [`csharp-external-references`](csharp-external-references) | random creates, updates and deletes of links with external references (raw numbers) in the C# stores, compared with a plain list after every step |
| [`rust-external-references`](rust-external-references) | the same check for the Rust stores of doublets |
| [`rust-split-internal-references`](rust-split-internal-references) | the same check for the Rust stores of doublets with internal references only, with and without references to links that are created later |
| [`csharp-usage-index-trees`](csharp-usage-index-trees) | the C# united store's index tree types with the set links of the usage benchmarks (all members share one source) |
| [`rust-plain-memory-capacity`](rust-plain-memory-capacity) | whether the Rust stores of doublets grow beyond 2^20 links with the plain memory of platform-mem, without [`Whole`](../rust/src/memory.rs) |
| [`check-full-size.sh`](check-full-size.sh) | the in-memory Doublets stores of both languages with the largest scheduled size |

External references
([`logs/csharp-external-references.log`](logs/csharp-external-references.log)):
with only external references every store is correct; when links mix
internal and external references, the `SplitMemoryLinks` of
Platform.Data.Doublets 0.18.1 loses links from its concrete index: after
`create 3=(2,18446744073709551595)` the query `(2, 18446744073709551595)`
finds nothing instead of link 3. The C# usage benchmarks keep their queue
nodes as `(next node, raw value)`, so they use the united store only.
The Rust `split::Store` of doublets 0.5.0 had the same defect (after
`update 1=(2, 1)` with an external target the incoming query of link 1 missed
it); doublets 0.8.0 passes both checks
([`logs/rust-external-references.log`](logs/rust-external-references.log)).
The Rust usage benchmarks still use the united store only, as the C# ones.

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

Internal references
([`logs/rust-split-internal-references.log`](logs/rust-split-internal-references.log)):
the Rust `split::Store` of doublets 0.5.0 lost links with internal references
too: after `update 3 = (4, 3)` the concrete query `(4, 3)` found nothing. The
`split::Store` of doublets 0.8.0 is correct as long as every link refers to
existing links, but it still loses a link that refers to an id before the link
with that id is created: after `update 4 = (6, 4)` and `create_point -> 6` the
concrete query `(6, 4)` finds nothing. The united store is correct in both
cases. So `rust/tests/same_behavior.rs` runs the random changes of the split
store with references to existing links only.

```bash
cd experiments/rust-split-internal-references
cargo run --release             # references to existing links and the next id
cargo run --release -- existing # references to existing links only
```

```bash
cd experiments/csharp-usage-index-trees
dotnet run -c Release -- 200000
```

Largest size ([`logs/check-full-size.log`](logs/check-full-size.log)): the
stores of doublets 0.5.0 to 0.8.0 fail after 1,040,384 links, so
[`check-full-size.sh`](check-full-size.sh) runs the Doublets benchmarks that
create links (Create, KeyValue Set and Queue Push) with the largest scheduled
size, 1,000,000 background links and 1,000 operations per iteration, under a
4 GiB address space limit for Rust and a 1 GiB heap limit for .NET. Every store
of both languages passes its result checks; in each group the Rust lines come
before the C# lines.

```bash
experiments/check-full-size.sh > experiments/logs/check-full-size.log
```

Plain memory
([`logs/rust-plain-memory-capacity.log`](logs/rust-plain-memory-capacity.log)):
with the `Global` memory of platform-mem 0.3.0 and without `Whole`, both
stores of doublets 0.8.0 still panic when they create link 1,040,384, so the
benchmarks keep [`Whole`](../rust/src/memory.rs).

```bash
cd experiments/rust-plain-memory-capacity
cargo run --release -- united
cargo run --release -- split
```
