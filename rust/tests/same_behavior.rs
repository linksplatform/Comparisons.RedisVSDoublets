//! Checks that every benchmarked store gives the same results for the
//! operations used by the benchmarks, and that undoing an operation (as the
//! benchmarks do after every iteration) restores the background data.
//!
//! - The links stores run the operations of the links benchmarks, and the
//!   unit store and Redis also run random sequences of changes with general
//!   links, compared with a model after every commit. The split store of
//!   doublets 0.5.0 does not find links with `source != target` by
//!   `[*, source, *]` or `[*, source, target]` (see
//!   `experiments/rust-external-references`), so it only gets point links
//!   and links with `source = target = 0`, as in the benchmarks.
//! - The [`Usage`] stores run the work of every usage benchmark and some edge
//!   cases, compared with a model of the Redis commands.
//!
//! The Redis tests need a server at `REDIS_URL` (default
//! `redis://localhost:6379`) whose database they empty:
//! `cargo test -- --include-ignored`.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use doublets::{
    Doublets, Error, Link,
    data::{Flow, LinksConstants},
};
use redis_vs_doublets::{
    Benched, DoubletsSplitVolatile, DoubletsUnitedVolatile, DoubletsUsage, Mode, Redis, RedisUsage,
    Snapshot, Usage,
};

const BACKGROUND: usize = 20;
const LINKS: usize = 5;

/// The Redis tests share one database, so they run one after another.
fn redis_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn any() -> usize {
    LinksConstants::<usize>::new().any
}

/// Returns the links matching `query`, sorted by id.
fn query<B: Doublets<usize>>(store: &B, query: [usize; 3]) -> Vec<Link<usize>> {
    let mut links = Vec::new();
    store.each_by(query, |link| {
        links.push(link);
        Flow::Continue
    });
    links.sort_by_key(|link| link.index);
    links
}

fn all_links<B: Doublets<usize>>(store: &B) -> Vec<Link<usize>> {
    query(store, [any(); 3])
}

fn points(ids: impl IntoIterator<Item = usize>) -> Vec<Link<usize>> {
    ids.into_iter().map(Link::point).collect()
}

// ============================================================================
// LINKS
// ============================================================================

/// Runs the operations of the links benchmarks inside `begin`/`commit` and
/// checks their results after the commit.
fn check_links<B: Benched + Doublets<usize>>(benched: &mut B) {
    let any = any();
    let mut store = benched.fork(BACKGROUND).unwrap();
    let background = points(1..=BACKGROUND);
    assert_eq!(all_links(&*store), background);

    // Create, undone by Delete.
    store.begin().unwrap();
    let created: Vec<_> = (0..LINKS).map(|_| store.create_point().unwrap()).collect();
    store.commit().unwrap();
    assert_eq!(
        created,
        (BACKGROUND + 1..=BACKGROUND + LINKS).collect::<Vec<_>>()
    );
    assert_eq!(all_links(&*store), points(1..=BACKGROUND + LINKS));
    store.begin().unwrap();
    for id in (BACKGROUND + 1..=BACKGROUND + LINKS).rev() {
        store.delete(id).unwrap();
    }
    store.commit().unwrap();
    assert_eq!(all_links(&*store), background);

    // Delete, undone by Create.
    store.begin().unwrap();
    for id in (BACKGROUND - LINKS + 1..=BACKGROUND).rev() {
        store.delete(id).unwrap();
    }
    store.commit().unwrap();
    assert_eq!(all_links(&*store), points(1..=BACKGROUND - LINKS));
    assert_eq!(store.get_link(BACKGROUND), None);
    store.begin().unwrap();
    for _ in 0..LINKS {
        store.create_point().unwrap();
    }
    store.commit().unwrap();
    assert_eq!(all_links(&*store), background);

    // Update: the first update of every link, then the second one, which
    // restores it.
    let updated = BACKGROUND - LINKS + 1..=BACKGROUND;
    store.begin().unwrap();
    for id in updated.clone() {
        store.update(id, 0, 0).unwrap();
    }
    store.commit().unwrap();
    let zeros: Vec<_> = updated.clone().map(|id| Link::new(id, 0, 0)).collect();
    assert_eq!(
        store.get_link(BACKGROUND),
        Some(Link::new(BACKGROUND, 0, 0))
    );
    assert_eq!(
        query(&*store, [BACKGROUND, any, any]),
        [Link::new(BACKGROUND, 0, 0)]
    );
    assert_eq!(query(&*store, [any, BACKGROUND, any]), []);
    assert_eq!(query(&*store, [any, any, BACKGROUND]), []);
    let mut expected = points(1..BACKGROUND - LINKS + 1);
    expected.extend(zeros);
    assert_eq!(all_links(&*store), expected);
    store.begin().unwrap();
    for id in updated.clone() {
        store.update(id, id, id).unwrap();
    }
    store.commit().unwrap();
    assert_eq!(all_links(&*store), background);

    // The work of the Update benchmark in one iteration.
    store.begin().unwrap();
    for id in updated {
        store.update(id, 0, 0).unwrap();
        store.update(id, id, id).unwrap();
    }
    store.commit().unwrap();
    assert_eq!(all_links(&*store), background);

    // The queries of the benchmarks.
    for id in 1..=BACKGROUND {
        let point = vec![Link::point(id)];
        assert_eq!(query(&*store, [id, any, any]), point);
        assert_eq!(query(&*store, [any, id, id]), point);
        assert_eq!(query(&*store, [any, id, any]), point);
        assert_eq!(query(&*store, [any, any, id]), point);
        assert_eq!(store.count_by([id, any, any]), 1);
        assert_eq!(store.count_by([any, id, id]), 1);
        assert_eq!(store.count_by([any, id, any]), 1);
        assert_eq!(store.count_by([any, any, id]), 1);
    }
    let missing = BACKGROUND + 1;
    assert_eq!(query(&*store, [missing, any, any]), []);
    assert_eq!(query(&*store, [any, missing, missing]), []);
    assert_eq!(query(&*store, [any, 1, 2]), []);
    assert_eq!(query(&*store, [any, missing, any]), []);
    assert_eq!(query(&*store, [any, any, missing]), []);
    assert_eq!(store.count_by([missing, any, any]), 0);
    assert_eq!(store.count(), BACKGROUND);
}

/// Checks that changing a missing link is an error: at once in the
/// non-transaction mode and in Doublets, at the commit in the other modes.
fn check_missing<B: Benched + Doublets<usize>>(benched: &mut B, batch: bool) {
    let mut store = benched.fork(BACKGROUND).unwrap();
    let missing = BACKGROUND + 1;
    for change in [
        |store: &mut B, id| store.delete(id).map(drop),
        |store: &mut B, id| store.update(id, 1, 1).map(drop),
    ] {
        store.begin().unwrap();
        let result = change(&mut store, missing);
        if batch {
            assert!(result.is_ok());
            assert!(store.commit().is_err());
        } else {
            assert!(
                matches!(result, Err(Error::NotExists(id)) if id == missing),
                "{result:?}"
            );
            store.commit().unwrap();
        }
    }
    assert_eq!(all_links(&*store), points(1..=BACKGROUND));
}

#[test]
fn doublets_united_volatile() {
    let mut store = DoubletsUnitedVolatile::setup(()).unwrap();
    check_links(&mut store);
    check_missing(&mut store, false);
    check_random(&mut store);
}

#[test]
fn doublets_split_volatile() {
    let mut store = DoubletsSplitVolatile::setup(()).unwrap();
    check_links(&mut store);
    check_missing(&mut store, false);
}

/// All modes share one database, so they run one after another in one test.
#[test]
#[ignore = "needs a Redis server at REDIS_URL"]
fn redis_links() {
    let _lock = redis_lock();
    for mode in Mode::ALL {
        let mut store = Redis::<usize>::setup(mode).unwrap();
        assert_eq!(store.mode(), mode);
        check_links(&mut store);
        check_missing(&mut store, mode != Mode::NonTransaction);
        check_random(&mut store);
        check_sink(&mut store);
    }
}

/// Checks that the queries queued in the transaction and pipeline modes pass
/// the found links to the sink at the commit, as the handler gets them in the
/// non-transaction mode.
fn check_sink(redis: &mut Redis<usize>) {
    let any = any();
    let found = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&found);
    redis.set_sink(move |link| sink.lock().unwrap().push(link));
    let mut store = redis.fork(BACKGROUND).unwrap();
    let queries: [fn(usize, usize) -> [usize; 3]; 5] = [
        |id, any| [id, any, any],
        |id, any| [any, id, id],
        |id, any| [any, id, any],
        |id, any| [any, any, id],
        |_, any| [any, any, any],
    ];
    for (i, query) in queries.into_iter().enumerate() {
        let mut handled = Vec::new();
        store.begin();
        for id in 1..=LINKS {
            store.each_by(query(id, any), |link| {
                handled.push(link);
                Flow::Continue
            });
        }
        store.commit().unwrap();
        let mut found = std::mem::take(&mut *found.lock().unwrap());
        if store.mode() == Mode::NonTransaction {
            assert_eq!(found, []);
            found = handled;
        } else {
            assert_eq!(handled, []);
        }
        found.sort_by_key(|link| link.index);
        let mut expected = if i == 4 {
            (0..LINKS).flat_map(|_| points(1..=BACKGROUND)).collect()
        } else {
            points(1..=LINKS)
        };
        expected.sort_by_key(|link| link.index);
        assert_eq!(found, expected, "query {i} in {:?}", store.mode());
    }
}

/// A deterministic pseudo-random number generator (64-bit LCG).
struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % bound as u64) as usize
    }
}

/// The links that a store must contain.
#[derive(Default)]
struct LinksModel {
    links: BTreeMap<usize, (usize, usize)>,
}

impl LinksModel {
    fn has_pair(&self, source: usize, target: usize) -> bool {
        self.links.values().any(|&pair| pair == (source, target))
    }

    fn next_id(&self) -> usize {
        self.links.keys().next_back().map_or(1, |id| id + 1)
    }

    fn matching(&self, filter: impl Fn(usize, usize, usize) -> bool) -> Vec<Link<usize>> {
        self.links
            .iter()
            .filter(|&(&id, &(source, target))| filter(id, source, target))
            .map(|(&id, &(source, target))| Link::new(id, source, target))
            .collect()
    }

    /// Checks every query of the benchmarks for every id, and the counts.
    fn check<B: Doublets<usize>>(&self, store: &B) {
        let any = any();
        assert_eq!(all_links(store), self.matching(|_, _, _| true));
        assert_eq!(store.count(), self.links.len());
        for id in 1..=self.next_id() {
            let identity = self.matching(|i, _, _| i == id);
            let outgoing = self.matching(|_, source, _| source == id);
            let incoming = self.matching(|_, _, target| target == id);
            assert_eq!(query(store, [id, any, any]), identity, "[{id}, *, *]");
            assert_eq!(query(store, [any, id, any]), outgoing, "[*, {id}, *]");
            assert_eq!(query(store, [any, any, id]), incoming, "[*, *, {id}]");
            assert_eq!(store.count_by([any, id, any]), outgoing.len());
            assert_eq!(store.count_by([any, any, id]), incoming.len());
            if let Some(&(source, target)) = self.links.get(&id) {
                assert_eq!(store.get_link(id), Some(Link::new(id, source, target)));
                assert_eq!(query(store, [id, source, target]), identity);
                assert_eq!(query(store, [id, source + 1, target]), []);
                if (source, target) != (0, 0) {
                    assert_eq!(query(store, [any, source, target]), identity);
                    assert_eq!(store.count_by([any, source, target]), 1);
                }
            } else {
                assert_eq!(store.get_link(id), None);
            }
            assert_eq!(
                query(store, [any, id, id + 1]),
                self.matching(|_, source, target| (source, target) == (id, id + 1))
            );
        }
    }
}

/// Runs random creates, updates and deletes in `begin`/`commit` and compares
/// all links and the results of all queries with a model after every commit.
///
/// The pairs stay unique (doublets treats a pair as unique) and only the link
/// with the highest id is deleted, so both databases assign the same ids (see
/// [`redis_vs_doublets::redis_impl`]).
fn check_random<B: Benched + Doublets<usize>>(benched: &mut B) {
    let mut store = benched.fork(BACKGROUND).unwrap();
    let mut model = LinksModel::default();
    model
        .links
        .extend((1..=BACKGROUND).map(|id| (id, (id, id))));
    let mut random = Random(42);
    for _ in 0..30 {
        store.begin().unwrap();
        for _ in 0..10 {
            let next = model.next_id();
            let (source, target) = (1 + random.below(next), 1 + random.below(next));
            let pair_is_free = !model.has_pair(source, target);
            match random.below(5) {
                0 => {
                    assert_eq!(store.create_point().unwrap(), next);
                    model.links.insert(next, (next, next));
                }
                1 if pair_is_free && source < next && target < next => {
                    assert_eq!(store.create_link(source, target).unwrap(), next);
                    model.links.insert(next, (source, target));
                }
                2 | 3 if pair_is_free && next > 1 => {
                    let id = 1 + random.below(next - 1);
                    store.update(id, source, target).unwrap();
                    model.links.insert(id, (source, target));
                }
                4 if next > 1 => {
                    store.delete(next - 1).unwrap();
                    model.links.remove(&(next - 1));
                }
                _ => {}
            }
        }
        store.commit().unwrap();
        model.check(&*store);
    }
}

// ============================================================================
// TYPICAL REDIS USAGE
// ============================================================================

/// One command of the typical Redis usage.
#[derive(Clone, Copy, Debug)]
enum Op {
    Set(u64, u64),
    Get(u64),
    Delete(u64),
    Increment(u64),
    Push(u64),
    Pop,
    PushFront(u64),
    PopBack,
    Add(u64),
    Remove(u64),
    Contains(u64),
}

/// The Redis commands on plain collections: what every [`Usage`] store must
/// do.
struct UsageModel {
    values: BTreeMap<u64, u64>,
    queue: VecDeque<u64>,
    members: BTreeSet<u64>,
}

impl UsageModel {
    fn background(background: usize) -> Self {
        let numbers = 1..=background as u64;
        Self {
            values: numbers.clone().map(|number| (number, number)).collect(),
            queue: numbers.clone().collect(),
            members: numbers.collect(),
        }
    }

    /// Runs `op` and returns the value passed to the sink, if any.
    fn run(&mut self, op: Op) -> Option<u64> {
        match op {
            Op::Set(key, value) => drop(self.values.insert(key, value)),
            Op::Get(key) => return self.values.get(&key).copied(),
            Op::Delete(key) => drop(self.values.remove(&key)),
            Op::Increment(key) => *self.values.entry(key).or_default() += 1,
            Op::Push(value) => self.queue.push_back(value),
            Op::Pop => return self.queue.pop_front(),
            Op::PushFront(value) => self.queue.push_front(value),
            Op::PopBack => drop(self.queue.pop_back()),
            Op::Add(member) => drop(self.members.insert(member)),
            Op::Remove(member) => drop(self.members.remove(&member)),
            Op::Contains(member) => return Some(self.members.contains(&member) as u64),
        }
        None
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            values: self
                .values
                .iter()
                .map(|(&key, &value)| (key, value))
                .collect(),
            queue: self.queue.iter().copied().collect(),
            members: self.members.iter().copied().collect(),
        }
    }
}

fn run<U: Usage>(store: &mut U, op: Op) {
    match op {
        Op::Set(key, value) => store.set(key, value),
        Op::Get(key) => store.get(key),
        Op::Delete(key) => store.delete(key),
        Op::Increment(key) => store.increment(key),
        Op::Push(value) => store.push(value),
        Op::Pop => store.pop(),
        Op::PushFront(value) => store.push_front(value),
        Op::PopBack => store.pop_back(),
        Op::Add(member) => store.add(member),
        Op::Remove(member) => store.remove(member),
        Op::Contains(member) => store.contains(member),
    }
    .unwrap_or_else(|error| panic!("{op:?} failed: {error}"));
}

/// The work of every usage benchmark and its undo (see
/// `benches/benchmarks/usage.rs`), followed by edge cases.
fn scenario() -> Vec<(&'static str, Vec<Op>)> {
    let (b, n) = (BACKGROUND as u64, LINKS as u64);
    let first = || 1..=n;
    let last = || (b - n + 1..=b).rev();
    vec![
        ("KeyValue_Get", first().map(Op::Get).collect()),
        ("KeyValue_Set", first().map(|k| Op::Set(k, k + b)).collect()),
        (
            "KeyValue_Set undo",
            first().map(|k| Op::Set(k, k)).collect(),
        ),
        ("KeyValue_Delete", last().map(Op::Delete).collect()),
        (
            "KeyValue_Delete undo",
            last().map(|k| Op::Set(k, k)).collect(),
        ),
        ("Counter_Increment", first().map(Op::Increment).collect()),
        (
            "Counter_Increment undo",
            first().map(|k| Op::Set(k, k)).collect(),
        ),
        ("Queue_Push", first().map(|i| Op::Push(b + i)).collect()),
        ("Queue_Push undo", first().map(|_| Op::PopBack).collect()),
        ("Queue_Pop", first().map(|_| Op::Pop).collect()),
        ("Queue_Pop undo", first().rev().map(Op::PushFront).collect()),
        ("Set_Add", first().map(|i| Op::Add(b + i)).collect()),
        ("Set_Add undo", first().map(|i| Op::Remove(b + i)).collect()),
        ("Set_Remove", last().map(Op::Remove).collect()),
        ("Set_Remove undo", last().map(Op::Add).collect()),
        ("Set_Contains", first().map(Op::Contains).collect()),
        // Missing keys and members, and values that are already there.
        (
            "missing",
            vec![
                Op::Get(b + 1),
                Op::Delete(b + 1),
                Op::Remove(b + 1),
                Op::Contains(b + 1),
                Op::Add(1),
                Op::Contains(1),
            ],
        ),
        (
            "increment a missing key",
            vec![Op::Increment(b + 1), Op::Increment(b + 1), Op::Get(b + 1)],
        ),
        (
            "set a value twice",
            vec![
                Op::Set(b + 1, 7),
                Op::Set(b + 1, 9),
                Op::Get(b + 1),
                Op::Delete(b + 1),
            ],
        ),
        (
            "large numbers",
            vec![
                Op::Set(1 << 40, u32::MAX as u64),
                Op::Get(1 << 40),
                Op::Add(1 << 40),
                Op::Increment(1 << 40),
            ],
        ),
        ("undo", vec![Op::Delete(1 << 40), Op::Remove(1 << 40)]),
        // The queue is emptied, popped while empty, and filled again.
        ("empty the queue", (0..=b).map(|_| Op::Pop).collect()),
        ("pop an empty queue", vec![Op::Pop, Op::PopBack, Op::Pop]),
        (
            "push to an empty queue",
            vec![
                Op::Push(3),
                Op::PopBack,
                Op::PushFront(2),
                Op::Push(3),
                Op::PushFront(1),
            ],
        ),
        ("drain", vec![Op::Pop, Op::Pop, Op::Pop, Op::Pop]),
        ("refill", (1..=b).map(Op::Push).collect()),
        // Several steps in one iteration, as in a real program.
        (
            "mixed",
            vec![
                Op::Set(1, 100),
                Op::Increment(1),
                Op::Get(1),
                Op::Push(b + 1),
                Op::Pop,
                Op::Add(b + 1),
                Op::Contains(b + 1),
                Op::Remove(b + 1),
                Op::Contains(b + 1),
            ],
        ),
    ]
}

/// Runs every step of [`scenario`] in `begin`/`commit` and compares the data
/// and the values passed to the sink with [`UsageModel`] after every commit.
fn check_usage<U: Usage>(benched: &mut U) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    benched.set_sink(move |value| sink.lock().unwrap().push(value));
    let mut store = benched.fork(BACKGROUND).unwrap();
    let mut model = UsageModel::background(BACKGROUND);
    assert_eq!(store.snapshot().unwrap(), Snapshot::background(BACKGROUND));
    for (name, ops) in scenario() {
        let mut expected = Vec::new();
        store.begin().unwrap();
        for &op in &ops {
            run(&mut *store, op);
            expected.extend(model.run(op));
        }
        store.commit().unwrap();
        let received = std::mem::take(&mut *received.lock().unwrap());
        assert_eq!(received, expected, "values of {name}");
        assert_eq!(
            store.snapshot().unwrap(),
            model.snapshot(),
            "data after {name}"
        );
        // Every benchmark iteration starts from the background data.
        if name.ends_with(" undo") || matches!(name, "KeyValue_Get" | "Set_Contains") {
            assert_eq!(model.snapshot(), Snapshot::background(BACKGROUND), "{name}");
        }
    }
    drop(store);
    // The fork removed all data.
    assert_eq!(benched.snapshot().unwrap(), Snapshot::default());
}

#[test]
fn doublets_usage() {
    let mut store = DoubletsUsage::setup(()).unwrap();
    check_usage(&mut store);
    // A second fork of the same store starts from the background data again.
    check_usage(&mut store);
}

/// All modes share one database, so they run one after another in one test.
#[test]
#[ignore = "needs a Redis server at REDIS_URL"]
fn redis_usage() {
    let _lock = redis_lock();
    for mode in Mode::ALL {
        let mut store = RedisUsage::setup(mode).unwrap();
        assert_eq!(store.mode(), mode);
        check_usage(&mut store);
    }
}
