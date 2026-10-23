//! # Doublets Implementation
//!
//! This module documents how the [doublets](https://crates.io/crates/doublets)
//! stores implement the [`Doublets`](doublets::Doublets) interface. The stores
//! run inside the benchmark process and keep their data in RAM: every
//! operation reads and writes the store's memory directly, without a server,
//! a network connection or a command protocol.
//!
//! ## Stores
//!
//! A link is identified by its position in an array: link `id` is element
//! `id` of the array. Two size-balanced trees index the links, one ordered by
//! `(source, target)` and one ordered by `(target, source)`. The tree nodes
//! are stored in the array elements as well, so no memory is allocated per
//! link.
//!
//! ### United (unit) store
//!
//! One array; each element holds the link and its nodes of both trees:
//!
//! ```text
//! LinkPart: source, target,
//!           left_as_source, right_as_source, size_as_source,
//!           left_as_target, right_as_target, size_as_target
//! ```
//!
//! ```
//! use doublets::Doublets;
//! use redis_vs_doublets::{Benched, DoubletsUnitedVolatile};
//!
//! let mut store: DoubletsUnitedVolatile = Benched::setup(())?;
//! let id = store.create_point()?;
//! assert_eq!(store.get_link(id).map(|link| (link.source, link.target)), Some((id, id)));
//! # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! ```
//!
//! ### Split store
//!
//! Two arrays: the link data and the tree nodes are kept apart, so reading
//! links does not load tree nodes into the CPU cache:
//!
//! ```text
//! DataPart:  source, target
//! IndexPart: root_as_source, left_as_source, right_as_source, size_as_source,
//!            root_as_target, left_as_target, right_as_target, size_as_target
//! ```
//!
//! ```
//! use doublets::Doublets;
//! use redis_vs_doublets::{Benched, DoubletsSplitVolatile};
//!
//! let mut store: DoubletsSplitVolatile = Benched::setup(())?;
//! let id = store.create_point()?;
//! assert_eq!(store.get_link(id).map(|link| (link.source, link.target)), Some((id, id)));
//! # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! ```
//!
//! The arrays are kept in RAM (`Global`, "Volatile"), as Redis keeps its
//! data, so neither side writes to a file during the benchmarks.
//!
//! ## Operations
//!
//! `n` is the number of links, `k` the number of found links.
//!
//! | Operation       | Method                       | Implementation                                                        | Time         |
//! |-----------------|------------------------------|-----------------------------------------------------------------------|--------------|
//! | Create          | `create_point()`             | take a free element (a deleted one or the next one), write `(id, id)`, insert into both trees | O(log n) |
//! | Update          | `update(id, source, target)` | remove from both trees, write the new values, insert into both trees  | O(log n)     |
//! | Delete          | `delete(id)`                 | update to `(0, 0)`, then mark the element as free                     | O(log n)     |
//! | Each All        | `each([*, *, *])`            | scan the array, skipping free elements                                | O(n)         |
//! | Each Identity   | `each_by([id, *, *])`        | read element `id`                                                     | O(1)         |
//! | Each Concrete   | `each_by([*, source, target])` | search the `(source, target)` tree                                  | O(log n)     |
//! | Each Outgoing   | `each_by([*, source, *])`    | walk the `(source, target)` tree from the first link with `source`    | O(log n + k) |
//! | Each Incoming   | `each_by([*, *, target])`    | walk the `(target, source)` tree from the first link with `target`    | O(log n + k) |
//!
//! Links with `source = target = 0` are not inserted into the trees.
//!
//! A concrete query returns at most one link, because doublets treats a
//! `(source, target)` pair as unique. The benchmarks never create two links
//! with the same pair.
//!
//! ## Capacity
//!
//! A store reserves space for `2^20` array elements in steps: first `8192`,
//! then the rest, and later more. `platform-mem` 0.3.0 returns only the newly
//! added part of the memory from every step after the first one, and a
//! doublets 0.5.0 store uses that part as its whole array, so without a
//! workaround a store holds at most `2^20 - 8192 - 1` links. The stores of the
//! benchmarks get their memory through [`Whole`](crate::Whole), which returns
//! the whole memory, so they grow beyond `2^20` links (checked by
//! `tests/doublets_capacity.rs`).
