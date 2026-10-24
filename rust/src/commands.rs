//! The connection to Redis shared by [`Redis`](crate::Redis) and
//! [`RedisUsage`](crate::RedisUsage), and the handling of the [`Mode`].
//!
//! The connection is opened once and reused by all operations. In
//! [`Mode::NonTransaction`] every command is sent at once and its reply is read
//! before the operation returns. In the other modes the commands between
//! [`Commands::begin`] and [`Commands::commit`] are queued with a tag that says
//! what to do with the reply, and [`Commands::commit`] sends them in one round
//! trip and hands every reply with its tag to the caller, in the order of the
//! operations.

use redis::{Cmd, Connection, ErrorKind, Pipeline, RedisError, RedisResult, Value};

/// How the commands of one benchmark iteration are sent to Redis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Every operation sends its command and waits for the reply before the
    /// next operation: one round trip each.
    NonTransaction,
    /// The commands of an iteration are queued between `MULTI` and `EXEC` and
    /// run atomically at the commit: one round trip for the whole iteration.
    Transaction,
    /// The commands of an iteration are sent together at the commit, without
    /// waiting for each reply (pipelining), and run one by one, not
    /// atomically.
    Pipeline,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::NonTransaction, Mode::Transaction, Mode::Pipeline];

    /// The name of the benchmark of this mode, for example `Redis_Pipeline`.
    pub fn name(self) -> &'static str {
        match self {
            Mode::NonTransaction => "Redis_NonTransaction",
            Mode::Transaction => "Redis_Transaction",
            Mode::Pipeline => "Redis_Pipeline",
        }
    }
}

/// Number of links, keys, list items or set members created by one command
/// when the background data is created.
pub const BATCH_SIZE: usize = 10_000;

/// The functions of `redis/links.lua`, shared with the C# benchmarks.
pub const FUNCTIONS: &str = include_str!("../../redis/links.lua");

/// A connection and the commands queued on it, each with a tag `R` that says
/// what to do with its reply.
pub(crate) struct Commands<R> {
    connection: Connection,
    mode: Mode,
    pipeline: Pipeline,
    replies: Vec<R>,
    batch: bool,
}

impl<R> Commands<R> {
    /// Connects to `url` and loads the functions of `redis/links.lua`.
    pub fn connect(url: &str, mode: Mode) -> RedisResult<Self> {
        let mut connection = redis::Client::open(url)?.get_connection()?;
        redis::cmd("FUNCTION")
            .arg("LOAD")
            .arg("REPLACE")
            .arg(FUNCTIONS)
            .exec(&mut connection)?;
        let mut pipeline = redis::pipe();
        if mode == Mode::Transaction {
            pipeline.atomic();
        }
        Ok(Self {
            connection,
            mode,
            pipeline,
            replies: Vec::new(),
            batch: false,
        })
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Runs `command` at once, also between [`Commands::begin`] and
    /// [`Commands::commit`], where it does not see the queued commands.
    pub fn run(&mut self, command: &Cmd) -> RedisResult<Value> {
        command.query(&mut self.connection)
    }

    /// Runs `command` at once and returns its reply with `reply`, or queues
    /// it until [`Commands::commit`] and returns `None`.
    pub fn send(&mut self, command: Cmd, reply: R) -> RedisResult<Option<(R, Value)>> {
        if self.batch {
            self.pipeline.add_command(command);
            self.replies.push(reply);
            Ok(None)
        } else {
            Ok(Some((reply, self.run(&command)?)))
        }
    }

    /// Starts queueing the commands, unless the mode is
    /// [`Mode::NonTransaction`].
    pub fn begin(&mut self) {
        self.batch = self.mode != Mode::NonTransaction;
    }

    /// Sends the queued commands and passes every reply with its tag to
    /// `handle`. An error reply of any command is returned as an error.
    pub fn commit(
        &mut self,
        mut handle: impl FnMut(R, Value) -> RedisResult<()>,
    ) -> RedisResult<()> {
        self.batch = false;
        if self.pipeline.is_empty() {
            return Ok(());
        }
        let values = self.pipeline.query::<Vec<Value>>(&mut self.connection);
        self.pipeline.clear();
        let replies = std::mem::take(&mut self.replies);
        let values = values?;
        if values.len() != replies.len() {
            return Err(unexpected(
                "wrong number of replies",
                format!("{} replies to {} commands", values.len(), replies.len()),
            ));
        }
        replies
            .into_iter()
            .zip(values)
            .try_for_each(|(reply, value)| handle(reply, value))
    }

    /// Drops the queued commands and removes all keys of the database. The
    /// functions stay loaded.
    pub fn flush(&mut self) -> RedisResult<()> {
        self.batch = false;
        self.pipeline.clear();
        self.replies.clear();
        redis::cmd("FLUSHDB").exec(&mut self.connection)
    }
}

pub(crate) fn unexpected(description: &'static str, detail: String) -> RedisError {
    (ErrorKind::UnexpectedReturnType, description, detail).into()
}

/// Reads a non-negative decimal number.
pub(crate) fn parse_number(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    bytes.iter().try_fold(0u64, |number, &byte| {
        let digit = byte.wrapping_sub(b'0');
        if digit > 9 {
            return None;
        }
        number.checked_mul(10)?.checked_add(digit as u64)
    })
}

/// The bytes of a string reply.
pub(crate) fn bytes(value: &Value) -> RedisResult<&[u8]> {
    match value {
        Value::BulkString(bytes) => Ok(bytes),
        Value::SimpleString(text) => Ok(text.as_bytes()),
        other => Err(unexpected("expected a string", format!("{other:?}"))),
    }
}

/// A number reply: an integer or a string of digits.
pub(crate) fn number(value: &Value) -> RedisResult<u64> {
    match value {
        Value::Int(number) if *number >= 0 => Ok(*number as u64),
        other => bytes(other)
            .ok()
            .and_then(parse_number)
            .ok_or_else(|| unexpected("expected a number", format!("{other:?}"))),
    }
}

/// The items of an array or a set reply; `Nil` has none.
pub(crate) fn items(value: Value) -> RedisResult<Vec<Value>> {
    match value {
        Value::Array(items) | Value::Set(items) => Ok(items),
        Value::Nil => Ok(Vec::new()),
        other => Err(unexpected("expected an array", format!("{other:?}"))),
    }
}

/// The fields and values of a hash: a map (RESP3) or a flat array (RESP2).
pub(crate) fn pairs(value: Value) -> RedisResult<Vec<(Value, Value)>> {
    match value {
        Value::Map(pairs) => Ok(pairs),
        Value::Array(items) if items.len() % 2 == 0 => {
            let mut items = items.into_iter();
            let mut pairs = Vec::with_capacity(items.len() / 2);
            while let (Some(field), Some(value)) = (items.next(), items.next()) {
                pairs.push((field, value));
            }
            Ok(pairs)
        }
        other => Err(unexpected("expected a hash", format!("{other:?}"))),
    }
}

/// Splits `first..=last` into ranges of [`BATCH_SIZE`] numbers, so that no
/// command gets too large.
pub(crate) fn chunks(first: u64, last: u64) -> impl Iterator<Item = (u64, u64)> {
    (first..=last)
        .step_by(BATCH_SIZE)
        .map(move |start| (start, last.min(start + BATCH_SIZE as u64 - 1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_parsed() {
        assert_eq!(parse_number(b"0"), Some(0));
        assert_eq!(parse_number(b"18446744073709551615"), Some(u64::MAX));
        assert_eq!(parse_number(b"18446744073709551616"), None);
        assert_eq!(parse_number(b""), None);
        assert_eq!(parse_number(b"-1"), None);
        assert_eq!(parse_number(b"1:2"), None);
    }

    #[test]
    fn hashes_are_read_in_both_protocols() {
        let field = |text: &str| Value::BulkString(text.as_bytes().to_vec());
        let resp2 = Value::Array(vec![field("1"), field("2")]);
        let resp3 = Value::Map(vec![(field("1"), field("2"))]);
        assert_eq!(pairs(resp2).unwrap(), pairs(resp3).unwrap());
        assert!(pairs(Value::Array(vec![field("1")])).is_err());
    }

    #[test]
    fn chunks_cover_the_range() {
        assert_eq!(chunks(1, 0).count(), 0);
        assert_eq!(chunks(1, 1).collect::<Vec<_>>(), [(1, 1)]);
        assert_eq!(
            chunks(1, 20_001).collect::<Vec<_>>(),
            [(1, 10_000), (10_001, 20_000), (20_001, 20_001)]
        );
    }
}
