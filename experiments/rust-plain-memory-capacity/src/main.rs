//! Checks whether the doublets stores grow beyond 2^20 links with the plain `Global` memory of
//! platform-mem, without the `Whole` wrapper of `rust/src/memory.rs`. Each store runs in its own
//! process (`cargo run --release -- united` or `-- split`), because a failing store panics.

use std::sync::atomic::{AtomicUsize, Ordering};

use doublets::{Doublets, data::LinksConstants, mem::Global, split, unit};

/// The number of the link being created, printed if the store panics.
static CREATING: AtomicUsize = AtomicUsize::new(0);

/// More links than fit into the first two memory steps of a store.
const LINKS: usize = (1 << 20) + 10_000;

fn check<D: Doublets<usize>>(name: &str, mut store: D) {
    for created in 1..=LINKS {
        CREATING.store(created, Ordering::Relaxed);
        if let Err(error) = store.create_point() {
            println!("{name}: create {created} failed: {error}");
            return;
        }
    }
    let any = LinksConstants::<usize>::new().any;
    let found = [1, 1 << 19, 1 << 20, LINKS]
        .iter()
        .all(|&id| store.count_by([any, id, id]) == 1);
    println!("{name}: {} links, all probes found: {found}", store.count());
}

fn main() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        println!(
            "panic while creating link {}",
            CREATING.load(Ordering::Relaxed)
        );
        default(info);
    }));
    match std::env::args().nth(1).as_deref() {
        Some("united") => check(
            "united",
            unit::Store::<usize, _>::new(Global::new()).unwrap(),
        ),
        Some("split") => check(
            "split",
            split::Store::<usize, _, _>::new(Global::new(), Global::new()).unwrap(),
        ),
        _ => eprintln!("usage: rust-plain-memory-capacity united|split"),
    }
}
