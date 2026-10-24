Still present in doublets 0.8.0 (platform-mem 0.3.0, Rust 1.99.1, Linux x86_64), and the split store is affected as well as the unit store. Without a wrapper around `Global`, both stores panic when they create link 1,040,384:

```
doublets 0.8.0, platform-mem 0.3.0:

$ cargo run --release -- united
panic while creating link 1040384
thread 'main' panicked at ~/.cargo/registry/src/.../doublets-0.8.0/src/mem/unit/store.rs:153:55:
Data part should be in data memory

$ cargo run --release -- split
panic while creating link 1040384
thread 'main' panicked at ~/.cargo/registry/src/.../doublets-0.8.0/src/mem/split/store.rs:172:14:
Index part should be in index memory
```

Reproduction: [Comparisons.RedisVSDoublets/experiments/rust-plain-memory-capacity](https://github.com/linksplatform/Comparisons.RedisVSDoublets/tree/issue-13-f67d791f1225/experiments/rust-plain-memory-capacity). The benchmarks there keep their `Whole` memory wrapper (`rust/src/memory.rs`), which returns the whole allocation from `grow`, as the workaround.
