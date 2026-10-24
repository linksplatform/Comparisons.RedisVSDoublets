//! Differential check of the doublets 0.8.0 stores with internal references only (default
//! constants): random creates, updates and deletes of links, compared after every step with a
//! plain map. Prints the operations and the first mismatch of each store.
//!
//! Usage: cargo run --release [-- existing]
//!
//! Without arguments a source or target may be the id of the next link (not created yet), as in
//! `rust/tests/same_behavior.rs`; with `existing` every source and target is an existing link.

use std::collections::BTreeMap;

use doublets::{Doublets, data::Flow, mem::Global, split, unit};

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

/// Returns the first query whose result differs from the model.
fn mismatch<D: Doublets<usize>>(
    links: &D,
    model: &BTreeMap<usize, (usize, usize)>,
) -> Option<String> {
    let any = links.constants().any;
    let ids = |f: &dyn Fn(&(usize, usize)) -> bool| -> Vec<usize> {
        model
            .iter()
            .filter(|(_, v)| f(v))
            .map(|(&k, _)| k)
            .collect()
    };
    if links.count() != model.len() {
        return Some(format!("count {} != {}", links.count(), model.len()));
    }
    for (&id, &(s, t)) in model {
        let checks = [
            ("concrete", [any, s, t], ids(&|v| *v == (s, t))),
            ("outgoing", [any, s, any], ids(&|v| v.0 == s)),
            ("incoming", [any, any, t], ids(&|v| v.1 == t)),
        ];
        for (name, query, expected) in checks {
            let got = each(links, query);
            if got != expected {
                return Some(format!(
                    "{name} query of link {id}=({s}, {t}): got {got:?}, expected {expected:?}"
                ));
            }
        }
    }
    None
}

fn check<D: Doublets<usize>>(name: &str, mut links: D, existing: bool) {
    let mut model = BTreeMap::<usize, (usize, usize)>::new();
    let mut random = Random(1);
    let mut log = Vec::new();
    for step in 1..=5000 {
        let next = model.keys().next_back().map_or(1, |id| id + 1);
        let bound = if existing { next - 1 } else { next };
        if model.is_empty() || random.next(4) == 0 {
            // Doublets keeps every (source, target) pair once, so a duplicate is skipped.
            if model.values().any(|&v| v == (next, next)) {
                continue;
            }
            let id = links.create_point().unwrap();
            model.insert(id, (id, id));
            log.push(format!("create_point -> {id}"));
        } else {
            let pair = (1 + random.next(bound), 1 + random.next(bound));
            if model.values().any(|&v| v == pair) {
                continue;
            }
            if random.next(3) == 0 {
                let id = next - 1;
                if model.values().any(|&(s, t)| s == id || t == id) || pair.0 == id || pair.1 == id
                {
                    continue;
                }
                links.delete(id).unwrap();
                model.remove(&id);
                log.push(format!("delete {id}"));
            } else {
                let id = 1 + random.next(next - 1);
                links.update(id, pair.0, pair.1).unwrap();
                model.insert(id, pair);
                log.push(format!("update {id} = {pair:?}"));
            }
        }
        if let Some(error) = mismatch(&links, &model) {
            println!(
                "{name}: after step {step} ({} operations): {error}",
                log.len()
            );
            for line in log.iter().rev().take(5).rev() {
                println!("  {line}");
            }
            return;
        }
    }
    println!("{name}: OK (5000 steps)");
}

fn main() {
    let existing = std::env::args().nth(1).as_deref() == Some("existing");
    println!(
        "references: {}",
        if existing {
            "existing links only"
        } else {
            "existing links and the next id"
        }
    );
    check(
        "united",
        unit::Store::<usize, _>::new(Global::new()).unwrap(),
        existing,
    );
    check(
        "split",
        split::Store::<usize, _, _>::new(Global::new(), Global::new()).unwrap(),
        existing,
    );
}
