//! Checks that the Doublets stores of the benchmarks grow beyond the limit of
//! doublets 0.5.0 with platform-mem 0.3.0 described in
//! `redis_vs_doublets::doublets_impl` (section "Capacity"): without
//! [`Whole`](redis_vs_doublets::Whole) a store holds at most
//! `2^20 - 8192 - 1` links.

use doublets::{Doublets, data::LinksConstants};
use redis_vs_doublets::{
    Benched, DoubletsSplitVolatile, DoubletsUnitedVolatile, DoubletsUsage, Usage,
};

/// More links than fit into the first two memory steps of a store.
const LINKS: usize = (1 << 20) + 10_000;

fn check<B: Benched + Doublets<usize>>(mut store: B) {
    let mut store = store.fork(LINKS).unwrap();
    assert_eq!(store.count(), LINKS);
    let any = LinksConstants::<usize>::new().any;
    for id in [1, 1 << 19, 1 << 20, LINKS] {
        assert_eq!(store.count_by([any, id, id]), 1, "link {id}");
    }
    assert_eq!(store.create_point().unwrap(), LINKS + 1);
}

#[test]
fn united_store_grows_beyond_the_limit() {
    check(DoubletsUnitedVolatile::setup(()).unwrap());
}

#[test]
fn split_store_grows_beyond_the_limit() {
    check(DoubletsSplitVolatile::setup(()).unwrap());
}

/// The usage store creates 3 links per background number.
#[test]
fn usage_store_grows_beyond_the_limit() {
    let mut store = DoubletsUsage::setup(()).unwrap();
    let background = 400_000;
    let mut store = store.fork(background).unwrap();
    let data = store.snapshot().unwrap();
    assert_eq!(data.values.len(), background);
    assert_eq!(data.queue.len(), background);
    assert_eq!(data.members.len(), background);
}
