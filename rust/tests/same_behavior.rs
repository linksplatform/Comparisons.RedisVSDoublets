use redis_vs_doublets::{DoubletsStore, Link, RedisStore, Store, exercise, undo};

fn snapshot(store: &mut dyn Store, query: [usize; 3]) -> Vec<Link> {
    let mut links = Vec::new();
    store.each(query, &mut |link| links.push(link)).unwrap();
    links.sort();
    links
}

fn check(store: &mut dyn Store) {
    let any = usize::MAX;
    for id in 1..=20 {
        assert_eq!(store.create().unwrap(), id);
    }
    let points: Vec<_> = (1..=20).map(|id| Link(id, id, id)).collect();
    assert_eq!(snapshot(store, [any; 3]), points);
    for op in 0..8 {
        exercise(store, op, 20, 5).unwrap();
        let expected: Vec<_> = match op {
            0 => (1..=25).map(|id| Link(id, id, id)).collect(),
            1 => (1..=20)
                .map(|id| {
                    if id <= 5 {
                        Link(id, id, id % 20 + 1)
                    } else {
                        Link(id, id, id)
                    }
                })
                .collect(),
            2 => (1..=15).map(|id| Link(id, id, id)).collect(),
            _ => points.clone(),
        };
        assert_eq!(snapshot(store, [any; 3]), expected, "operation {op}");
        undo(store, op, 20, 5).unwrap();
        assert_eq!(snapshot(store, [any; 3]), points, "undo operation {op}");
    }
    // Asymmetric links and shared endpoints exercise index maintenance.
    store.update(19, 1, 2).unwrap();
    store.update(20, 1, 3).unwrap();
    assert_eq!(snapshot(store, [19, any, any]), vec![Link(19, 1, 2)]);
    assert_eq!(snapshot(store, [any, 1, 2]), vec![Link(19, 1, 2)]);
    assert_eq!(
        snapshot(store, [any, 1, any]),
        vec![Link(1, 1, 1), Link(19, 1, 2), Link(20, 1, 3)]
    );
    assert_eq!(
        snapshot(store, [any, any, 2]),
        vec![Link(2, 2, 2), Link(19, 1, 2)]
    );
    store.update(19, 4, 5).unwrap();
    assert!(snapshot(store, [any, 1, 2]).is_empty());
    store.delete(20).unwrap();
    assert!(snapshot(store, [20, any, any]).is_empty());
    assert!(snapshot(store, [any, 1, 3]).is_empty());
    assert!(store.update(99, 1, 2).is_err());
    assert!(store.delete(99).is_err());
    assert!(snapshot(store, [any, 99, any]).is_empty());
}

#[test]
fn doublets_behavior() {
    check(&mut DoubletsStore::new().unwrap());
}

#[test]
#[ignore = "requires Redis; CI runs with --include-ignored"]
fn redis_behavior_and_isolation() {
    let mut first = RedisStore::new().unwrap();
    let mut second = RedisStore::new().unwrap();
    check(&mut first);
    assert!(snapshot(&mut second, [usize::MAX; 3]).is_empty());
    assert_eq!(second.create().unwrap(), 1);
    drop(first);
    assert_eq!(snapshot(&mut second, [usize::MAX; 3]), vec![Link(1, 1, 1)]);
}
