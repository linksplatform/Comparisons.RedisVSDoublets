//! Differential check of the doublets stores with external references (raw numbers):
//! random creates, updates and deletes of links, compared after every step with a plain map.
//! Prints the first mismatch of each store.
//!
//! Usage: cargo run --release [-- mixed]
//!
//! Without arguments every source and target is a raw number; with `mixed` half of them are
//! links. Raw numbers are `Hybrid::external(n)`, which is `usize::MAX - n + 1`.

use std::collections::BTreeMap;

use doublets::{
    Doublets,
    data::{Flow, Hybrid, LinksConstants},
    mem::Global,
    split, unit,
};

/// A small deterministic generator (xorshift), so the runs are reproducible without dependencies.
struct Random(u64);

impl Random {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn each<D: Doublets<usize>>(links: &D, query: [usize; 3]) -> Vec<usize> {
    let mut found = Vec::new();
    links.each_by(query, |link| {
        found.push(link.index);
        Flow::Continue
    });
    found.sort();
    found
}

fn check<D: Doublets<usize>>(name: &str, mut links: D, mixed: bool) {
    let any = links.constants().any;
    let mut model = BTreeMap::<usize, (usize, usize)>::new();
    let mut random = Random(1);
    let mut error = None;
    let mut step = 0;
    while step < 5000 && error.is_none() {
        step += 1;
        let value = |random: &mut Random, model: &BTreeMap<usize, (usize, usize)>| {
            if mixed && !model.is_empty() && random.next(2) == 0 {
                *model.keys().nth(random.next(model.len())).unwrap()
            } else {
                Hybrid::external(1 + random.next(29)).as_inner()
            }
        };
        let op = random.next(3);
        let what;
        if op == 0 || model.len() < 3 {
            let pair = (value(&mut random, &model), value(&mut random, &model));
            // Doublets keeps every (source, target) pair once, so a duplicate is skipped.
            if model.values().any(|&v| v == pair) {
                continue;
            }
            let id = links.create().unwrap();
            links.update(id, pair.0, pair.1).unwrap();
            model.insert(id, pair);
            what = format!("create {id}={pair:?}");
        } else {
            let id = *model.keys().nth(random.next(model.len())).unwrap();
            if op == 1 {
                let pair = (value(&mut random, &model), value(&mut random, &model));
                if model.values().any(|&v| v == pair) {
                    continue;
                }
                links.update(id, pair.0, pair.1).unwrap();
                model.insert(id, pair);
                what = format!("update {id}={pair:?}");
            } else {
                // A link that others refer to cannot be deleted; skip those.
                if model.values().any(|&(s, t)| s == id || t == id) {
                    continue;
                }
                links.delete(id).unwrap();
                model.remove(&id);
                what = format!("delete {id}");
            }
        }
        for (&id, &(s, t)) in &model {
            let expected = |f: &dyn Fn(&(usize, usize)) -> bool| -> Vec<usize> {
                model
                    .iter()
                    .filter(|(_, v)| f(v))
                    .map(|(&k, _)| k)
                    .collect()
            };
            let checks = [
                ("identity", each(&links, [id, any, any]), vec![id]),
                (
                    "concrete",
                    each(&links, [any, s, t]),
                    expected(&|v| *v == (s, t)),
                ),
                (
                    "outgoing",
                    each(&links, [any, s, any]),
                    expected(&|v| v.0 == s),
                ),
                (
                    "incoming",
                    each(&links, [any, any, t]),
                    expected(&|v| v.1 == t),
                ),
            ];
            for (query, got, expected) in checks {
                if got != expected {
                    error = Some(format!(
                        "after `{what}`: {query} of link {id}={:?}: got {got:?}, expected {expected:?}",
                        (s, t)
                    ));
                    break;
                }
            }
            if error.is_none() && links.count() != model.len() {
                error = Some(format!(
                    "after `{what}`: count {} != {}",
                    links.count(),
                    model.len()
                ));
            }
            if error.is_some() {
                break;
            }
        }
    }
    let kind = if mixed {
        "internal and external"
    } else {
        "external only"
    };
    println!(
        "{name} ({kind}): {} (step {step})",
        error.as_deref().unwrap_or("OK")
    );
}

fn main() {
    let mixed = std::env::args().nth(1).as_deref() == Some("mixed");
    let constants = LinksConstants::external;
    check(
        "united",
        unit::Store::<usize, _>::with_constants(Global::new(), constants()).unwrap(),
        mixed,
    );
    check(
        "split",
        split::Store::<usize, _, _>::with_constants(Global::new(), Global::new(), constants())
            .unwrap(),
        mixed,
    );
}
