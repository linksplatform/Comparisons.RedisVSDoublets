//! # Typical Redis usage
//!
//! The data structures that Redis is typically used for, behind the [`Usage`]
//! trait: a key-value cache with counters, a FIFO queue (a Redis list used as
//! a job queue) and a set. Keys, values and members are numbers from 1 to
//! [`MAX_NUMBER`] (`i64::MAX`, the largest value of a Redis counter):
//! [`DoubletsUsage`] returns an error for other numbers, because it cannot
//! store 0 or larger numbers as raw numbers.
//!
//! - [`RedisUsage`] keeps them in the native Redis types, so every operation
//!   is one native command.
//! - [`DoubletsUsage`] reimplements them in links of an in-memory Doublets
//!   store.
//!
//! Results (the value of `GET`, the popped value of `LPOP` and the answer of
//! `SISMEMBER`) are passed to the sink of the store ([`Usage::set_sink`]),
//! because Redis in the transaction and pipeline modes receives them only in
//! [`Benched::commit`].
//!
//! ## Redis
//!
//! | Key       | Type   | Commands                                  |
//! |-----------|--------|-------------------------------------------|
//! | `k:{key}` | string | `SET`, `GET`, `DEL`, `INCR`               |
//! | `queue`   | list   | `RPUSH`, `LPOP`, `LPUSH`, `RPOP`          |
//! | `members` | set    | `SADD`, `SREM`, `SISMEMBER`               |
//!
//! ## Doublets
//!
//! Numbers are stored as external references (raw numbers, see
//! [`Hybrid`](doublets::data::Hybrid)), which never collide with link ids.
//! Doublets keeps every `(source, target)` pair once and indexes links by
//! source and by target, so a lookup by key or by member is a search of an
//! index tree, and every other step follows a link by its id.
//!
//! | Structure | Links                                                              |
//! |-----------|--------------------------------------------------------------------|
//! | key-value | the link `(key, value)`; only these links have a raw number as the source, so `GET key` is the search `(*, key, *)` |
//! | set       | the link `(set, member)`, where `set` is a point link created by the fork; `SISMEMBER` is the search of the pair `(set, member)` |
//! | queue     | a singly linked list: the node of a value is the link `(next, value)`, and the head link `queue = (first, last)` points to its ends; the last node points to `queue` instead of a next node, and an empty queue is the point `(queue, queue)` |
//!
//! `RPUSH` and `LPOP` follow ids only; `RPOP` (only used to undo `RPUSH`)
//! finds the node before the last one by the search `(*, last, *)`. The order
//! of the updates keeps every pair unique at every step: a new node is created
//! as a point and gets its pair only when no other link can have it.
//!
//! Only the united store is used, as in C#: the `SplitMemoryLinks` of
//! Platform.Data.Doublets 0.18.1 does not find links whose source is a link
//! and whose target is a raw number, such as the set members, and neither did
//! the split store of doublets 0.5.0 (see `experiments/README.md`).

use std::hint::black_box;

use doublets::{
    Doublets, Links,
    data::{AddrToRaw, Flow, LinksConstants, RawToAddr},
};
use redis::{RedisResult, Value};

use crate::{
    Benched, DoubletsUnitedVolatile, Fork, Mode,
    commands::{Commands, chunks, items, number},
    memory::volatile,
    redis_url,
};

/// The typical Redis data structures. Every method is one Redis command.
pub trait Usage: Benched {
    /// Sets the value of `key`, creating the key if it does not exist
    /// (Redis `SET`).
    fn set(&mut self, key: u64, value: u64) -> crate::Result<()>;

    /// Passes the value of `key` to the sink if the key exists (Redis `GET`).
    fn get(&mut self, key: u64) -> crate::Result<()>;

    /// Deletes `key` if it exists (Redis `DEL`).
    fn delete(&mut self, key: u64) -> crate::Result<()>;

    /// Adds one to the value of `key`; a missing key counts from 0
    /// (Redis `INCR`).
    fn increment(&mut self, key: u64) -> crate::Result<()>;

    /// Appends `value` to the end of the queue (Redis `RPUSH`).
    fn push(&mut self, value: u64) -> crate::Result<()>;

    /// Removes the first value of the queue and passes it to the sink, if the
    /// queue is not empty (Redis `LPOP`).
    fn pop(&mut self) -> crate::Result<()>;

    /// Inserts `value` at the start of the queue (Redis `LPUSH`). Used to undo
    /// [`Usage::pop`].
    fn push_front(&mut self, value: u64) -> crate::Result<()>;

    /// Removes the last value of the queue, if any (Redis `RPOP`). Used to
    /// undo [`Usage::push`].
    fn pop_back(&mut self) -> crate::Result<()>;

    /// Adds `member` to the set if it is not there yet (Redis `SADD`).
    fn add(&mut self, member: u64) -> crate::Result<()>;

    /// Removes `member` from the set if it is there (Redis `SREM`).
    fn remove(&mut self, member: u64) -> crate::Result<()>;

    /// Passes 1 to the sink if the set contains `member`, otherwise 0
    /// (Redis `SISMEMBER`).
    fn contains(&mut self, member: u64) -> crate::Result<()>;

    /// Sets the function that receives the results. By default they are
    /// passed to [`black_box`].
    fn set_sink(&mut self, sink: impl FnMut(u64) + Send + Sync + 'static);

    /// Returns all data. Not benchmarked; it must not be called between
    /// [`Benched::begin`] and [`Benched::commit`].
    fn snapshot(&mut self) -> crate::Result<Snapshot>;
}

/// All data of a [`Usage`] store, for the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// The keys with their values, sorted by key.
    pub values: Vec<(u64, u64)>,
    /// The queue from its start.
    pub queue: Vec<u64>,
    /// The set members, sorted.
    pub members: Vec<u64>,
}

impl Snapshot {
    /// The data that [`Benched::fork`] creates: the keys `1..=background`,
    /// each with its own number as the value, the queue
    /// `1, 2, ..., background` and the set `{1, ..., background}`.
    pub fn background(background: usize) -> Self {
        let numbers: Vec<u64> = (1..=background as u64).collect();
        Self {
            values: numbers.iter().map(|&number| (number, number)).collect(),
            queue: numbers.clone(),
            members: numbers,
        }
    }
}

type Sink = Box<dyn FnMut(u64) + Send + Sync>;

fn default_sink() -> Sink {
    Box::new(|value| {
        black_box(value);
    })
}

// ============================================================================
// REDIS
// ============================================================================

/// What to do with the reply of a queued command.
enum Reply {
    /// Only checked for an error.
    Ignore,
    /// A value or `nil` (`GET`, `LPOP`).
    Value,
    /// 1 or 0 (`SISMEMBER`).
    Bool,
}

/// The typical Redis data structures in their native Redis types.
pub struct RedisUsage {
    commands: Commands<Reply>,
    sink: Sink,
}

const QUEUE: &str = "queue";
const MEMBERS: &str = "members";

fn key(key: u64) -> String {
    format!("k:{key}")
}

impl RedisUsage {
    /// Connects to `url`. The data in the database is not changed.
    pub fn connect(url: &str, mode: Mode) -> crate::Result<Self> {
        Ok(Self {
            commands: Commands::connect(url, mode)?,
            sink: default_sink(),
        })
    }

    pub fn mode(&self) -> Mode {
        self.commands.mode()
    }

    fn send(&mut self, command: redis::Cmd, reply: Reply) -> crate::Result<()> {
        if let Some((reply, value)) = self.commands.send(command, reply)? {
            handle(&mut self.sink, reply, value)?;
        }
        Ok(())
    }
}

fn handle(sink: &mut Sink, reply: Reply, value: Value) -> RedisResult<()> {
    match reply {
        Reply::Ignore => {}
        Reply::Value if value == Value::Nil => {}
        Reply::Value | Reply::Bool => sink(number(&value)?),
    }
    Ok(())
}

impl Benched for RedisUsage {
    type Builder<'a> = Mode;

    fn setup(mode: Self::Builder<'_>) -> crate::Result<Self> {
        let mut store = Self::connect(&redis_url(), mode)?;
        store.unfork()?;
        Ok(store)
    }

    /// Creates the background data with `MSET`, `RPUSH` and `SADD`,
    /// [`BATCH_SIZE`](crate::redis_impl::BATCH_SIZE) values per command.
    fn fork(&mut self, background: usize) -> crate::Result<Fork<'_, Self>> {
        for (first, last) in chunks(1, background as u64) {
            let mut set = redis::cmd("MSET");
            let mut push = redis::cmd("RPUSH");
            let mut add = redis::cmd("SADD");
            push.arg(QUEUE);
            add.arg(MEMBERS);
            for number in first..=last {
                set.arg(key(number)).arg(number);
                push.arg(number);
                add.arg(number);
            }
            for command in [set, push, add] {
                self.commands.run(&command)?;
            }
        }
        Ok(Fork(self))
    }

    fn begin(&mut self) -> crate::Result<()> {
        self.commands.begin();
        Ok(())
    }

    fn commit(&mut self) -> crate::Result<()> {
        let sink = &mut self.sink;
        Ok(self
            .commands
            .commit(|reply, value| handle(sink, reply, value))?)
    }

    /// Drops the queued commands and removes all keys (`FLUSHDB`).
    fn unfork(&mut self) -> crate::Result<()> {
        Ok(self.commands.flush()?)
    }
}

impl Usage for RedisUsage {
    fn set(&mut self, k: u64, value: u64) -> crate::Result<()> {
        let mut command = redis::cmd("SET");
        command.arg(key(k)).arg(value);
        self.send(command, Reply::Ignore)
    }

    fn get(&mut self, k: u64) -> crate::Result<()> {
        let mut command = redis::cmd("GET");
        command.arg(key(k));
        self.send(command, Reply::Value)
    }

    fn delete(&mut self, k: u64) -> crate::Result<()> {
        let mut command = redis::cmd("DEL");
        command.arg(key(k));
        self.send(command, Reply::Ignore)
    }

    fn increment(&mut self, k: u64) -> crate::Result<()> {
        let mut command = redis::cmd("INCR");
        command.arg(key(k));
        self.send(command, Reply::Ignore)
    }

    fn push(&mut self, value: u64) -> crate::Result<()> {
        let mut command = redis::cmd("RPUSH");
        command.arg(QUEUE).arg(value);
        self.send(command, Reply::Ignore)
    }

    fn pop(&mut self) -> crate::Result<()> {
        let mut command = redis::cmd("LPOP");
        command.arg(QUEUE);
        self.send(command, Reply::Value)
    }

    fn push_front(&mut self, value: u64) -> crate::Result<()> {
        let mut command = redis::cmd("LPUSH");
        command.arg(QUEUE).arg(value);
        self.send(command, Reply::Ignore)
    }

    fn pop_back(&mut self) -> crate::Result<()> {
        let mut command = redis::cmd("RPOP");
        command.arg(QUEUE);
        self.send(command, Reply::Ignore)
    }

    fn add(&mut self, member: u64) -> crate::Result<()> {
        let mut command = redis::cmd("SADD");
        command.arg(MEMBERS).arg(member);
        self.send(command, Reply::Ignore)
    }

    fn remove(&mut self, member: u64) -> crate::Result<()> {
        let mut command = redis::cmd("SREM");
        command.arg(MEMBERS).arg(member);
        self.send(command, Reply::Ignore)
    }

    fn contains(&mut self, member: u64) -> crate::Result<()> {
        let mut command = redis::cmd("SISMEMBER");
        command.arg(MEMBERS).arg(member);
        self.send(command, Reply::Bool)
    }

    fn set_sink(&mut self, sink: impl FnMut(u64) + Send + Sync + 'static) {
        self.sink = Box::new(sink);
    }

    fn snapshot(&mut self) -> crate::Result<Snapshot> {
        let numbers = |value: Value| -> crate::Result<Vec<u64>> {
            Ok(items(value)?
                .iter()
                .map(number)
                .collect::<RedisResult<_>>()?)
        };
        let mut values = Vec::new();
        for k in items(self.commands.run(redis::cmd("KEYS").arg("k:*"))?)? {
            let name = crate::commands::bytes(&k)?.to_vec();
            let number_of_key = crate::commands::parse_number(&name[2..])
                .ok_or_else(|| format!("unexpected key {}", String::from_utf8_lossy(&name)))?;
            let value = self.commands.run(redis::cmd("GET").arg(name))?;
            values.push((number_of_key, number(&value)?));
        }
        values.sort_unstable();
        let queue = numbers(
            self.commands
                .run(redis::cmd("LRANGE").arg(QUEUE).arg(0).arg(-1))?,
        )?;
        let mut members = numbers(self.commands.run(redis::cmd("SMEMBERS").arg(MEMBERS))?)?;
        members.sort_unstable();
        Ok(Snapshot {
            values,
            queue,
            members,
        })
    }
}

// ============================================================================
// DOUBLETS
// ============================================================================

/// The store of [`DoubletsUsage`]: the united store in RAM with external
/// references.
type Store = DoubletsUnitedVolatile<u64>;

/// The typical Redis data structures reimplemented in links of an in-memory
/// Doublets store.
pub struct DoubletsUsage {
    links: Store,
    set: u64,
    queue: u64,
    sink: Sink,
}

/// The largest number that [`DoubletsUsage`] stores: raw numbers use half of
/// the values of `u64`. It is also the largest value of a Redis counter
/// (`i64::MAX`).
pub const MAX_NUMBER: u64 = u64::MAX / 2;

/// The raw number (external reference) of `number`, which must be in
/// `1..=MAX_NUMBER` (the raw number of 0 is the null link).
pub fn raw(number: u64) -> u64 {
    AddrToRaw.convert(number)
}

/// The raw number of `number`, or an error if [`raw`] cannot represent it.
fn checked(number: u64) -> crate::Result<u64> {
    if (1..=MAX_NUMBER).contains(&number) {
        Ok(raw(number))
    } else {
        Err(format!("Doublets stores the numbers 1..={MAX_NUMBER}, not {number}").into())
    }
}

/// The number of the raw number (external reference) `raw`.
pub fn number_of(raw: u64) -> u64 {
    RawToAddr.convert(raw)
}

fn open() -> crate::Result<Store> {
    Ok(Store::with_constants(
        volatile(),
        LinksConstants::external(),
    )?)
}

impl DoubletsUsage {
    /// The name of its benchmarks.
    pub const NAME: &'static str = "Doublets_United_Volatile";

    /// The source and the target of the link `id`.
    fn link(&self, id: u64) -> crate::Result<(u64, u64)> {
        let link = self.links.try_get_link(id)?;
        Ok((link.source, link.target))
    }

    /// The first link that matches `(*, source, target)`.
    fn first(&self, source: u64, target: u64) -> Option<u64> {
        let mut found = None;
        self.links
            .each_by([self.links.constants().any, source, target], |link| {
                found = Some(link.index);
                Flow::Break
            });
        found
    }

    /// The link `(key, value)` of the raw number `key`.
    fn find(&self, key: u64) -> Option<u64> {
        self.first(key, self.links.constants().any)
    }

    /// The link `(set, member)` of the raw number `member`.
    fn find_member(&self, member: u64) -> Option<u64> {
        self.links.search(self.set, member)
    }
}

impl Benched for DoubletsUsage {
    type Builder<'a> = ();

    fn setup(_: Self::Builder<'_>) -> crate::Result<Self> {
        Ok(Self {
            links: open()?,
            set: 0,
            queue: 0,
            sink: default_sink(),
        })
    }

    /// Creates the set and the queue head links and then the background data.
    fn fork(&mut self, background: usize) -> crate::Result<Fork<'_, Self>> {
        self.set = self.links.create_point()?;
        self.queue = self.links.create_point()?;
        let numbers = 1..=background as u64;
        for number in numbers.clone() {
            self.links.create_link(raw(number), raw(number))?;
        }
        for number in numbers.clone() {
            self.push(number)?;
        }
        for number in numbers {
            self.links.create_link(self.set, raw(number))?;
        }
        Ok(Fork(self))
    }

    /// Replaces the store with a new, empty one.
    fn unfork(&mut self) -> crate::Result<()> {
        self.links = open()?;
        (self.set, self.queue) = (0, 0);
        Ok(())
    }
}

impl Usage for DoubletsUsage {
    fn set(&mut self, key: u64, value: u64) -> crate::Result<()> {
        let (key, value) = (checked(key)?, checked(value)?);
        match self.find(key) {
            Some(link) => self.links.update(link, key, value)?,
            None => self.links.create_link(key, value)?,
        };
        Ok(())
    }

    fn get(&mut self, key: u64) -> crate::Result<()> {
        if let Some(link) = self.find(checked(key)?) {
            let (_, value) = self.link(link)?;
            (self.sink)(number_of(value));
        }
        Ok(())
    }

    fn delete(&mut self, key: u64) -> crate::Result<()> {
        if let Some(link) = self.find(checked(key)?) {
            self.links.delete(link)?;
        }
        Ok(())
    }

    fn increment(&mut self, key: u64) -> crate::Result<()> {
        let key = checked(key)?;
        match self.find(key) {
            Some(link) => {
                let (_, value) = self.link(link)?;
                self.links
                    .update(link, key, checked(number_of(value) + 1)?)?
            }
            None => self.links.create_link(key, raw(1))?,
        };
        Ok(())
    }

    fn push(&mut self, value: u64) -> crate::Result<()> {
        let value = checked(value)?;
        let queue = self.queue;
        let (first, last) = self.link(queue)?;
        let node = self.links.create_point()?;
        if first == queue {
            self.links.update(node, queue, value)?;
            self.links.update(queue, node, node)?;
        } else {
            let (_, last_value) = self.link(last)?;
            self.links.update(last, node, last_value)?;
            self.links.update(node, queue, value)?;
            self.links.update(queue, first, node)?;
        }
        Ok(())
    }

    fn pop(&mut self) -> crate::Result<()> {
        let queue = self.queue;
        let (first, last) = self.link(queue)?;
        if first == queue {
            return Ok(());
        }
        let (next, value) = self.link(first)?;
        if next == queue {
            self.links.update(queue, queue, queue)?;
        } else {
            self.links.update(queue, next, last)?;
        }
        self.links.delete(first)?;
        (self.sink)(number_of(value));
        Ok(())
    }

    fn push_front(&mut self, value: u64) -> crate::Result<()> {
        let value = checked(value)?;
        let queue = self.queue;
        let (first, last) = self.link(queue)?;
        let node = self.links.create_point()?;
        self.links.update(node, first, value)?;
        self.links
            .update(queue, node, if first == queue { node } else { last })?;
        Ok(())
    }

    fn pop_back(&mut self) -> crate::Result<()> {
        let queue = self.queue;
        let (first, last) = self.link(queue)?;
        if first == queue {
            return Ok(());
        }
        if first == last {
            self.links.update(queue, queue, queue)?;
            self.links.delete(last)?;
            return Ok(());
        }
        // The node before the last one is the only link whose source is the
        // last node: the head link starts at the first node, and all other
        // links start at a raw number or at the set.
        let previous = self
            .first(last, self.links.constants().any)
            .ok_or("the queue has no node before its last one")?;
        self.links.update(queue, first, previous)?;
        self.links.delete(last)?;
        let (_, previous_value) = self.link(previous)?;
        self.links.update(previous, queue, previous_value)?;
        Ok(())
    }

    fn add(&mut self, member: u64) -> crate::Result<()> {
        let member = checked(member)?;
        if self.find_member(member).is_none() {
            self.links.create_link(self.set, member)?;
        }
        Ok(())
    }

    fn remove(&mut self, member: u64) -> crate::Result<()> {
        if let Some(link) = self.find_member(checked(member)?) {
            self.links.delete(link)?;
        }
        Ok(())
    }

    fn contains(&mut self, member: u64) -> crate::Result<()> {
        let found = self.find_member(checked(member)?).is_some();
        (self.sink)(found as u64);
        Ok(())
    }

    fn set_sink(&mut self, sink: impl FnMut(u64) + Send + Sync + 'static) {
        self.sink = Box::new(sink);
    }

    fn snapshot(&mut self) -> crate::Result<Snapshot> {
        let constants = self.links.constants().clone();
        let mut values = Vec::new();
        let mut members = Vec::new();
        self.links.each(|link| {
            if constants.is_external(link.source) {
                values.push((number_of(link.source), number_of(link.target)));
            } else if link.source == self.set && constants.is_external(link.target) {
                members.push(number_of(link.target));
            }
            Flow::Continue
        });
        values.sort_unstable();
        members.sort_unstable();
        let mut queue = Vec::new();
        if self.queue != 0 {
            let (mut node, _) = self.link(self.queue)?;
            while node != self.queue {
                let (next, value) = self.link(node)?;
                queue.push(number_of(value));
                node = next;
            }
        }
        Ok(Snapshot {
            values,
            queue,
            members,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_numbers_round_trip() {
        let constants = LinksConstants::<u64>::external();
        for number in [1, 2, 1000, 1 << 40, MAX_NUMBER] {
            let raw = raw(number);
            assert!(constants.is_external(raw), "{number} -> {raw}");
            assert!(!constants.is_internal(raw));
            assert_eq!(number_of(raw), number);
        }
    }

    #[test]
    fn numbers_outside_the_raw_range_are_rejected() {
        assert_eq!(checked(1).unwrap(), raw(1));
        assert_eq!(checked(MAX_NUMBER).unwrap(), raw(MAX_NUMBER));
        assert!(checked(0).is_err());
        assert!(checked(MAX_NUMBER + 1).is_err());

        let mut store = DoubletsUsage::setup(()).unwrap();
        let mut store = store.fork(1).unwrap();
        assert!(store.set(0, 1).is_err());
        assert!(store.set(1, 0).is_err());
        assert!(store.add(u64::MAX).is_err());
        store.set(1, MAX_NUMBER).unwrap();
        assert!(store.increment(1).is_err());
        let data = store.snapshot().unwrap();
        assert_eq!(data.values, [(1, MAX_NUMBER)]);
        assert_eq!((data.queue, data.members), (vec![1], vec![1]));
    }
}
