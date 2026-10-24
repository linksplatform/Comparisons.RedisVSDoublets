## Summary

`split::Store` of doublets 0.8.0 loses a link from its indexes when the link
refers to an id whose link is created only later. Once that link exists, a
concrete query for the pair finds nothing, although `get_link` still returns
the link. `unit::Store` finds it.

## Minimal reproduction (doublets 0.8.0, Rust 1.99.1, Linux x86_64)

`Cargo.toml`:

```toml
[package]
name = "split-forward-reference"
version = "0.1.0"
edition = "2024"

[dependencies]
doublets = "0.8.0"
```

`src/main.rs`:

```rust
use doublets::{Doublets, data::Flow, mem::Global, split, unit};

fn run<D: Doublets<usize>>(name: &str, mut links: D) {
    let _a = links.create_point().unwrap(); // 1 = (1, 1)
    let b = links.create_point().unwrap(); // 2 = (2, 2)
    links.update(b, 3, b).unwrap(); // 2 = (3, 2): link 3 does not exist yet
    let c = links.create_point().unwrap(); // 3 = (3, 3)
    let any = links.constants().any;
    let mut found = Vec::new();
    links.each_by([any, c, b], |link| {
        found.push(link.index);
        Flow::Continue
    });
    println!("{name}: each_by([any, 3, 2]) -> {found:?}, get_link(2) -> {:?}", links.get_link(b));
}

fn main() {
    run("united", unit::Store::<usize, _>::new(Global::new()).unwrap());
    run("split", split::Store::<usize, _, _>::new(Global::new(), Global::new()).unwrap());
}
```

Output:

```
united: each_by([any, 3, 2]) -> [2], get_link(2) -> Some(2: 3 2)
split: each_by([any, 3, 2]) -> [], get_link(2) -> Some(2: 3 2)
```

Expected: `split` returns `[2]`, as `united` does.

## Notes

- doublets 0.5.0 also lost links whose references all existed (for example
  after `update 3 = (4, 3)`); 0.8.0 fixes that case, and only the reference to a
  not yet created id remains. The differential check, its log for both
  versions and the commands are in
  [Comparisons.RedisVSDoublets/experiments/rust-split-internal-references](https://github.com/linksplatform/Comparisons.RedisVSDoublets/tree/issue-13-f67d791f1225/experiments/rust-split-internal-references).
- A likely cause is that the split store chooses the index tree (internal or
  external) of a source or target from whether that id exists when the link is
  written, and does not move the link when the id is created. If such
  references are not supported, `update`/`create` could return an error
  instead of keeping a link that queries cannot find.

## Workaround downstream

`Comparisons.RedisVSDoublets` runs the random changes of its split-store test
with references to existing links only, and does not delete a link that
others refer to (`rust/tests/same_behavior.rs`, `References::Existing`).
