//! # Redis Implementation
//!
//! [`Redis`] implements the same [`Doublets`] interface as the Doublets stores,
//! so every benchmark calls exactly the same methods on both databases.
//!
//! ## Client and connection
//!
//! Redis is reached through [redis-rs](https://crates.io/crates/redis), the
//! Rust client listed by Redis, with its synchronous connection: every
//! operation waits for its reply, as the operations of the Doublets stores
//! do.
//!
//! - The connection is opened once in [`Redis::connect`] and reused by all
//!   operations, so no TCP connection is opened per operation.
//! - All settings of the client and of the server are defaults.
//! - [`Redis::clear`] runs `FLUSHDB`: the benchmarks and the tests empty the
//!   database selected by `REDIS_URL`, so it must keep no other data.
//!
//! ## Data model
//!
//! A link is kept in one hash and indexed by three more keys, so every query
//! is a single native read command (`redis/links.lua`, shared with the C#
//! benchmarks):
//!
//! | Key                      | Type | Content                | Query                                   |
//! |--------------------------|------|------------------------|-----------------------------------------|
//! | `links`                  | hash | `id -> "source:target"`| `[id, *, *]` `HGET`, `[*, *, *]` `HGETALL` |
//! | `out:{source}`           | hash | `id -> target`         | `[*, source, *]` `HGETALL`              |
//! | `in:{target}`            | hash | `id -> source`         | `[*, *, target]` `HGETALL`              |
//! | `pair:{source}:{target}` | set  | ids                    | `[*, source, target]` `SMEMBERS`        |
//!
//! A change touches the link and all its indexes, so it runs as a Redis
//! function (`FCALL`): atomically, in one round trip, without sending the old
//! source and target to the client first.
//!
//! | Operation      | Command                                   |
//! |----------------|-------------------------------------------|
//! | `create_point` | `FCALL link_create 0 id id id`            |
//! | `update`       | `FCALL link_update 0 id source target`, returns the old `"source:target"` |
//! | `delete`       | `FCALL link_delete 0 id`, returns the old `"source:target"` |
//! | `each_by`      | `HGET`, `SMEMBERS` or `HGETALL`, see above |
//! | `count_by`     | `HLEN`, `HEXISTS`, `SCARD`, or `HGET` for `[id, source, target]` |
//!
//! New ids come from a counter kept by the client, as with a database
//! sequence: `1, 2, 3, ...`. Doublets also assigns `1, 2, 3, ...` to the links
//! of an empty store. As in Doublets, deleting the link with the highest id
//! makes that id the next one to be created. Ids of other deleted links are not
//! reused; the benchmarks only delete the links with the highest ids, so both
//! databases always assign the same ids.
//!
//! ## Modes
//!
//! | [`Mode`]                 | What happens                                                         |
//! |--------------------------|----------------------------------------------------------------------|
//! | [`Mode::NonTransaction`] | Every operation is one round trip; its result is known when it returns. |
//! | [`Mode::Transaction`]    | The commands between [`Redis::begin`] and [`Redis::commit`] are queued in `MULTI`/`EXEC`. |
//! | [`Mode::Pipeline`]       | The commands between [`Redis::begin`] and [`Redis::commit`] are pipelined. |
//!
//! In the transaction and pipeline modes the replies arrive only in
//! [`Redis::commit`], so between [`Redis::begin`] and [`Redis::commit`]:
//!
//! - `each_by` queues its command and returns [`Flow::Continue`] without
//!   calling its handler; the found links are passed to the sink
//!   ([`Redis::set_sink`]) in [`Redis::commit`]. A [`Flow::Break`] of a sink
//!   cannot stop a query that has already run.
//! - `update` and `delete` report [`Link::nothing`] as the state before the
//!   change, and an error of a change (such as a missing link) is returned by
//!   [`Redis::commit`].
//! - `get_link` and `count_by` run at once and do not see the queued changes.
//!
//! Errors returned by Redis are never ignored: methods that return `Result`
//! return them, and methods of the [`Links`] trait that cannot return an error
//! (`each_links`, `count_links`) panic, which stops the benchmark.

use std::{
    hint::black_box,
    sync::{Mutex, MutexGuard},
};

use doublets::{
    Doublets, Error, Link, Links,
    data::{Flow, LinkReference, LinksConstants, ReadHandler, WriteHandler},
};
use redis::{Cmd, RedisError, RedisResult, Value};

pub use crate::commands::{BATCH_SIZE, Mode};
use crate::commands::{Commands, bytes, chunks, items, number, pairs, parse_number, unexpected};

/// What to do with the reply of a queued command.
enum Reply<T> {
    /// A change; its reply only has to be checked for an error.
    Change,
    /// `HGET links id`: the link, if it matches the source and the target
    /// (`None` matches any).
    Identity {
        id: T,
        source: Option<T>,
        target: Option<T>,
    },
    /// `SMEMBERS pair:{source}:{target}`.
    Concrete { source: T, target: T },
    /// `HGETALL out:{source}`.
    Outgoing { source: T },
    /// `HGETALL in:{target}`.
    Incoming { target: T },
    /// `HGETALL links`.
    All,
}

/// Receives the links found by queued queries in [`Redis::commit`].
type Sink<T> = Box<dyn FnMut(Link<T>) + Send + Sync>;

/// A links store kept in Redis.
pub struct Redis<T: LinkReference> {
    commands: Mutex<Commands<Reply<T>>>,
    constants: LinksConstants<T>,
    next_id: T,
    sink: Sink<T>,
}

impl<T: LinkReference> Redis<T> {
    /// Connects to `url` and loads the functions of `redis/links.lua`. The
    /// data in the database is not changed.
    pub fn connect(url: &str, mode: Mode) -> crate::Result<Self> {
        Ok(Self {
            commands: Mutex::new(Commands::connect(url, mode)?),
            constants: LinksConstants::new(),
            next_id: T::from_byte(1),
            sink: Box::new(|link| {
                black_box(link);
            }),
        })
    }

    pub fn mode(&self) -> Mode {
        self.lock().mode()
    }

    /// Sets the function that receives the links found by the queries that
    /// are queued in the transaction and pipeline modes. By default the links
    /// are passed to [`black_box`].
    pub fn set_sink(&mut self, sink: impl FnMut(Link<T>) + Send + Sync + 'static) {
        self.sink = Box::new(sink);
    }

    /// Creates `count` point links with the next ids, [`BATCH_SIZE`] links per
    /// call of the function `link_points`.
    ///
    /// Used to prepare the background links outside of the measured time.
    pub fn create_points(&mut self, count: usize) -> crate::Result<()> {
        if count == 0 {
            return Ok(());
        }
        let first = to_u64(self.next_id);
        let last = first + count as u64 - 1;
        let commands = self.commands.get_mut().expect("commands mutex is poisoned");
        for (start, end) in chunks(first, last) {
            commands.run(
                redis::cmd("FCALL")
                    .arg("link_points")
                    .arg(0)
                    .arg(start)
                    .arg(end),
            )?;
        }
        self.next_id = from_u64(last + 1);
        Ok(())
    }

    /// Starts queueing the commands, unless the mode is
    /// [`Mode::NonTransaction`].
    pub fn begin(&mut self) {
        self.lock().begin();
    }

    /// Sends the queued commands, if any, and passes the links found by the
    /// queued queries to the sink.
    pub fn commit(&mut self) -> crate::Result<()> {
        let sink = &mut self.sink;
        let commands = self.commands.get_mut().expect("commands mutex is poisoned");
        Ok(commands.commit(|reply, value| {
            visit(reply, value, &mut |link| {
                sink(link);
                Flow::Continue
            })
            .map(drop)
        })?)
    }

    /// Drops the queued commands and removes all links (`FLUSHDB`).
    pub fn clear(&mut self) -> crate::Result<()> {
        self.commands
            .get_mut()
            .expect("commands mutex is poisoned")
            .flush()?;
        self.next_id = T::from_byte(1);
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, Commands<Reply<T>>> {
        self.commands.lock().expect("commands mutex is poisoned")
    }

    /// Runs a change, or queues it.
    fn change(&mut self, command: Cmd) -> RedisResult<Option<Value>> {
        let commands = self.commands.get_mut().expect("commands mutex is poisoned");
        Ok(commands
            .send(command, Reply::Change)?
            .map(|(_, value)| value))
    }

    /// The query `[index, source, target]` with `any` for the missing parts.
    fn parts(&self, query: &[T]) -> (T, T, T) {
        let any = self.constants.any;
        let part = |i: usize| query.get(i).copied().unwrap_or(any);
        (part(0), part(1), part(2))
    }

    fn identity(&self, id: T, source: T, target: T) -> Reply<T> {
        let any = self.constants.any;
        let filter = |part: T| (part != any).then_some(part);
        Reply::Identity {
            id,
            source: filter(source),
            target: filter(target),
        }
    }

    /// The command and the reply handling of a query.
    fn query(&self, query: &[T]) -> (Cmd, Reply<T>) {
        let any = self.constants.any;
        let (id, source, target) = self.parts(query);
        if id != any {
            let mut command = redis::cmd("HGET");
            command.arg("links").arg(to_u64(id));
            (command, self.identity(id, source, target))
        } else if source != any && target != any {
            let mut command = redis::cmd("SMEMBERS");
            command.arg(pair_key(source, target));
            (command, Reply::Concrete { source, target })
        } else if source != any {
            let mut command = redis::cmd("HGETALL");
            command.arg(format!("out:{}", to_u64(source)));
            (command, Reply::Outgoing { source })
        } else if target != any {
            let mut command = redis::cmd("HGETALL");
            command.arg(format!("in:{}", to_u64(target)));
            (command, Reply::Incoming { target })
        } else {
            let mut command = redis::cmd("HGETALL");
            command.arg("links");
            (command, Reply::All)
        }
    }
}

impl<T: LinkReference> Links<T> for Redis<T> {
    fn constants(&self) -> &LinksConstants<T> {
        &self.constants
    }

    fn count_links(&self, query: &[T]) -> T {
        let any = self.constants.any;
        let (id, source, target) = self.parts(query);
        let mut command;
        if id != any {
            let mut found = 0;
            visit(
                self.identity(id, source, target),
                self.lock()
                    .run(redis::cmd("HGET").arg("links").arg(to_u64(id)))
                    .expect("Redis count query failed"),
                &mut |_| {
                    found += 1;
                    Flow::Continue
                },
            )
            .expect("Redis count query failed");
            return from_u64(found);
        } else if source != any && target != any {
            command = redis::cmd("SCARD");
            command.arg(pair_key(source, target));
        } else if source != any {
            command = redis::cmd("HLEN");
            command.arg(format!("out:{}", to_u64(source)));
        } else if target != any {
            command = redis::cmd("HLEN");
            command.arg(format!("in:{}", to_u64(target)));
        } else {
            command = redis::cmd("HLEN");
            command.arg("links");
        }
        let count = self.lock().run(&command).expect("Redis count query failed");
        from_u64(number(&count).expect("Redis count query failed"))
    }

    fn create_links(
        &mut self,
        _query: &[T],
        handler: WriteHandler<'_, T>,
    ) -> Result<Flow, Error<T>> {
        let id = self.next_id;
        let null = self.constants.null;
        self.change(link_create(id, null, null)).map_err(other)?;
        self.next_id = id + T::from_byte(1);
        Ok(handler(Link::nothing(), Link::new(id, null, null)))
    }

    fn each_links(&self, query: &[T], handler: ReadHandler<'_, T>) -> Flow {
        let (command, reply) = self.query(query);
        let sent = self
            .lock()
            .send(command, reply)
            .expect("Redis read query failed");
        match sent {
            Some((reply, value)) => visit(reply, value, handler).expect("Redis read query failed"),
            None => Flow::Continue,
        }
    }

    fn update_links(
        &mut self,
        query: &[T],
        change: &[T],
        handler: WriteHandler<'_, T>,
    ) -> Result<Flow, Error<T>> {
        let (id, source, target) = (query[0], change[1], change[2]);
        let mut command = redis::cmd("FCALL");
        command
            .arg("link_update")
            .arg(0)
            .arg(to_u64(id))
            .arg(to_u64(source))
            .arg(to_u64(target));
        let before = match self
            .change(command)
            .map_err(|error| not_exists(error, id))?
        {
            Some(old) => old_link(id, &old).map_err(other)?,
            None => Link::nothing(),
        };
        Ok(handler(before, Link::new(id, source, target)))
    }

    fn delete_links(
        &mut self,
        query: &[T],
        handler: WriteHandler<'_, T>,
    ) -> Result<Flow, Error<T>> {
        let id = query[0];
        let mut command = redis::cmd("FCALL");
        command.arg("link_delete").arg(0).arg(to_u64(id));
        let before = match self
            .change(command)
            .map_err(|error| not_exists(error, id))?
        {
            Some(old) => old_link(id, &old).map_err(other)?,
            None => Link::nothing(),
        };
        if id + T::from_byte(1) == self.next_id {
            self.next_id = id;
        }
        Ok(handler(before, Link::nothing()))
    }
}

impl<T: LinkReference> Doublets<T> for Redis<T> {
    /// Reads the link at once, also between [`Redis::begin`] and
    /// [`Redis::commit`].
    fn get_link(&self, index: T) -> Option<Link<T>> {
        let value = self
            .lock()
            .run(redis::cmd("HGET").arg("links").arg(to_u64(index)))
            .expect("Redis read query failed");
        let mut found = None;
        let reply = Reply::Identity {
            id: index,
            source: None,
            target: None,
        };
        visit(reply, value, &mut |link| {
            found = Some(link);
            Flow::Break
        })
        .expect("Redis read query failed");
        found
    }

    /// Creates the point link with one function call instead of the default
    /// `create` followed by `update`.
    fn create_point(&mut self) -> Result<T, Error<T>> {
        let id = self.next_id;
        self.change(link_create(id, id, id)).map_err(other)?;
        self.next_id = id + T::from_byte(1);
        Ok(id)
    }
}

fn link_create<T: LinkReference>(id: T, source: T, target: T) -> Cmd {
    let mut command = redis::cmd("FCALL");
    command
        .arg("link_create")
        .arg(0)
        .arg(to_u64(id))
        .arg(to_u64(source))
        .arg(to_u64(target));
    command
}

fn pair_key<T: LinkReference>(source: T, target: T) -> String {
    format!("pair:{}:{}", to_u64(source), to_u64(target))
}

/// Passes the links of the reply of a query to `handler` until it returns
/// [`Flow::Break`]. A [`Reply::Change`] has no links.
fn visit<T: LinkReference>(
    reply: Reply<T>,
    value: Value,
    handler: &mut dyn FnMut(Link<T>) -> Flow,
) -> RedisResult<Flow> {
    let id_of = |value: &Value| number(value).map(from_u64::<T>);
    match reply {
        Reply::Change => Ok(Flow::Continue),
        Reply::Identity { id, source, target } => {
            if value == Value::Nil {
                return Ok(Flow::Continue);
            }
            let link = old_link(id, &value)?;
            let matches = |part: T, wanted: Option<T>| wanted.is_none_or(|wanted| part == wanted);
            Ok(
                if matches(link.source, source) && matches(link.target, target) {
                    handler(link)
                } else {
                    Flow::Continue
                },
            )
        }
        Reply::Concrete { source, target } => {
            for id in items(value)? {
                if handler(Link::new(id_of(&id)?, source, target)).is_break() {
                    return Ok(Flow::Break);
                }
            }
            Ok(Flow::Continue)
        }
        Reply::Outgoing { source } => {
            for (id, target) in pairs(value)? {
                if handler(Link::new(id_of(&id)?, source, id_of(&target)?)).is_break() {
                    return Ok(Flow::Break);
                }
            }
            Ok(Flow::Continue)
        }
        Reply::Incoming { target } => {
            for (id, source) in pairs(value)? {
                if handler(Link::new(id_of(&id)?, id_of(&source)?, target)).is_break() {
                    return Ok(Flow::Break);
                }
            }
            Ok(Flow::Continue)
        }
        Reply::All => {
            for (id, link) in pairs(value)? {
                if handler(old_link(id_of(&id)?, &link)?).is_break() {
                    return Ok(Flow::Break);
                }
            }
            Ok(Flow::Continue)
        }
    }
}

/// Reads the `"source:target"` value of the link `id`.
fn old_link<T: LinkReference>(id: T, value: &Value) -> RedisResult<Link<T>> {
    let text = bytes(value)?;
    let colon = text.iter().position(|&byte| byte == b':');
    let parsed = colon.and_then(|colon| {
        Some((
            parse_number(&text[..colon])?,
            parse_number(&text[colon + 1..])?,
        ))
    });
    match parsed {
        Some((source, target)) => Ok(Link::new(id, from_u64(source), from_u64(target))),
        None => Err(unexpected(
            "expected \"source:target\"",
            format!("{value:?}"),
        )),
    }
}

fn to_u64<T: LinkReference>(id: T) -> u64 {
    id.try_into().expect("link id does not fit into u64")
}

fn from_u64<T: LinkReference>(id: u64) -> T {
    T::try_from(id).expect("Redis returned an id that does not fit into the link type")
}

/// The error of a function that did not find the link `id`.
fn not_exists<T: LinkReference>(error: RedisError, id: T) -> Error<T> {
    if error.to_string().contains("does not exist") {
        Error::NotExists(id)
    } else {
        other(error)
    }
}

fn other<T: LinkReference>(error: RedisError) -> Error<T> {
    Error::Other(Box::new(error))
}
