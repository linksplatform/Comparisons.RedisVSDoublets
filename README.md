# Comparisons.RedisVSDoublets ([русская версия](README.ru.md))

[![Rust](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/rust.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/rust.yml)
[![C#](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/csharp.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/csharp.yml)
[![Benchmarks](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/benchmarks.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/benchmarks.yml)
[![CI quality](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/quality.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/quality.yml)

Comparison of [Redis](https://redis.io) and LinksPlatform's Doublets (links)
as in-memory storage, in Rust and in C#, made like
[Comparisons.SQLiteVSDoublets](https://github.com/linksplatform/Comparisons.SQLiteVSDoublets)
for [issue #1](https://github.com/linksplatform/Comparisons.RedisVSDoublets/issues/1).
Doublets runs in memory, without files, in the benchmark process; Redis runs as
a local server without persistence. Both are used through native libraries,
without an intermediate layer:

| Language | Redis client | Doublets |
| --- | --- | --- |
| Rust | [redis-rs](https://crates.io/crates/redis) (synchronous) | [doublets](https://crates.io/crates/doublets) 0.5.0 |
| C# | [StackExchange.Redis](https://www.nuget.org/packages/StackExchange.Redis) | [Platform.Data.Doublets](https://www.nuget.org/packages/Platform.Data.Doublets) 0.18.1 |

## Benchmarks

There are two suites, and both languages run the same work on the same data.
`B` is the number of background links (or keys, queue items and set members)
that the store holds before every iteration, and `N` is the number of
operations of one iteration.

### Links storage

Both databases store links `(id, source, target)` behind the same interface
(the `Doublets` trait of doublets in Rust, `IBenchedLinks` in C#), and every
store starts with the point links `1..=B`:

| Operation | Measured work of one iteration | Undone after the iteration by |
| --- | --- | --- |
| Create | `N` point links `B + 1..=B + N` | deleting them |
| Update | `N` × (`update(id, 0, 0)`, then `update(id, id, id)`) | nothing (the second update restores the link) |
| Delete | the links `B` down to `B - N + 1` | creating them again |
| Each All | one enumeration of all `B` links | nothing (read only) |
| Each Identity | `N` queries `[id, *, *]` for the ids `1..=N` | nothing (read only) |
| Each Concrete | `N` queries `[*, id, id]` | nothing (read only) |
| Each Outgoing | `N` queries `[*, id, *]` | nothing (read only) |
| Each Incoming | `N` queries `[*, *, id]` | nothing (read only) |

Redis keeps a link in one hash and indexes it by three more keys, so every
query is one native read command, and every change is one call of a Redis
function of [`redis/links.lua`](redis/links.lua), shared by both languages,
which updates the link and all its indexes atomically in one round trip:

| Key | Type | Content | Query |
| --- | --- | --- | --- |
| `links` | hash | id → `source:target` | `[id, *, *]` = `HGET`, `[*, *, *]` = `HGETALL` |
| `out:{source}` | hash | id → target | `[*, source, *]` = `HGETALL` |
| `in:{target}` | hash | id → source | `[*, *, target]` = `HGETALL` |
| `pair:{source}:{target}` | set | ids | `[*, source, target]` = `SMEMBERS` |

Doublets runs both of its in-memory stores: `Doublets_United_Volatile` (one
array of links with index trees) and `Doublets_Split_Volatile` (separate data
and index arrays).

### Typical Redis usage

The data structures that Redis is typically used for, reimplemented in links.
The store starts with the keys `1..=B` (each with its own number as the value),
the queue `1, ..., B` and the set `{1, ..., B}`:

| Operation | Measured work of one iteration | Undone after the iteration by |
| --- | --- | --- |
| KeyValue Get | `N` × `GET k` for the keys `1..=N` | nothing (read only) |
| KeyValue Set | `N` × `SET k k+B` for the keys `1..=N` | `SET k k` |
| KeyValue Delete | `N` × `DEL k` for the keys `B` down to `B - N + 1` | `SET k k` |
| Counter Increment | `N` × `INCR k` for the keys `1..=N` | `SET k k` |
| Queue Push | `N` × `RPUSH B+i` | `N` × `RPOP` |
| Queue Pop | `N` × `LPOP`, which returns `1..=N` | `LPUSH` of `N` down to `1` |
| Set Add | `N` × `SADD B+i` | `SREM B+i` |
| Set Remove | `N` × `SREM m` for the members `B` down to `B - N + 1` | `SADD m` |
| Set Contains | `N` × `SISMEMBER m` for the members `1..=N` | nothing (read only) |

Redis keeps them in its native types, so every operation is one native
command: the strings `k:{key}`, the list `queue` and the set `members`.
Doublets stores numbers as external references (raw numbers), which never
collide with link ids, and keeps:

- a key-value pair as the link `(key, value)`, so `GET key` is the index
  search `(*, key, *)`;
- a set member as the link `(set, member)`, where `set` is a point link, so
  `SISMEMBER` is the search of the pair `(set, member)`;
- the queue as a singly linked list: the node of a value is the link
  `(next, value)`, and the head link `queue = (first, last)` points to its
  ends, so `RPUSH` and `LPOP` follow link ids only.

Only the united store is used here: the split stores of both libraries lose
links that mix link ids and raw numbers (see [experiments](experiments)).
Keys, values and members are numbers from 1 to `i64::MAX`, the largest value
of a Redis counter; the Doublets stores reject other numbers.

### Redis modes

Every Redis benchmark runs in three modes, so the cost of the round trips is
visible:

| Implementation | A finished iteration |
| --- | --- |
| `Redis_NonTransaction` | every operation is one round trip, and its result is known when it returns |
| `Redis_Transaction` | the operations of the iteration are queued in `MULTI`/`EXEC` |
| `Redis_Pipeline` | the operations of the iteration are pipelined |

The measured time starts before the iteration begins and ends after it is
committed, so in the transaction and pipeline modes it includes sending the
queued commands and receiving all replies. Every found link and read value is
consumed (`black_box` in Rust, a checksum in C#).

### Method

- Before the measurement the store gets its background data, and after every
  iteration the undo puts it back into the same state, so every iteration
  starts from the same data. Neither the background data nor the undo is
  measured, and neither is the connection.
- Every benchmark checks its results outside the measured time: the store must
  hold exactly the background data before the first and after the last
  iteration, and one iteration before the measurement must find exactly the
  expected links or values (their number and the sum of their ids or values).
  A store that loses data or finds wrong results fails the run instead of
  reporting a time.
- Rust uses [Criterion](https://crates.io/crates/criterion); C# uses a harness
  with the same sampling, which prints the same `bencher` lines. Redis takes 10
  flat samples after 1 s of warm-up in 5 s; Doublets takes 100 samples after
  1 s of warm-up in 2 s. The tables show the median time of one iteration.
- Every Doublets cell compares its median with the fastest Redis mode (in
  bold): `× faster`, `× slower`, or `≈ same` if the medians differ by less than
  5% or their ±1 standard deviation ranges overlap.
- Every language, backend and size runs in its own GitHub Actions job (its own
  virtual machine), so they do not compete for the same CPU, and Redis runs in
  that job as a service container with its snapshots switched off. Pull
  requests check the whole pipeline with `B = 1,000` and `N = 100`; pushes to
  `main` measure `B = 10,000`, `100,000` and `1,000,000` with `N = 1,000` and
  update the results below. The workflow can also be started manually with
  other sizes.
- The provenance line of every table records the Redis, client, Doublets and
  compiler or SDK versions, the CPU, the commit, the date and the run.

### Limitations

- Redis time includes the client serialization and a local network round
  trip, while Doublets works in the benchmark process; this is the difference
  between the two access paths that the comparison measures. Remote networks,
  concurrent clients, memory usage and persistence are not measured.
- The benchmarks use a single standalone Redis server: the functions compute
  their keys from the link, so they do not run in a Redis Cluster.
- The benchmarks and the tests empty the Redis database of `REDIS_URL`
  (`FLUSHDB`), so it must be a disposable database.
- The stores of doublets 0.5.0 (Rust) fail after 1,040,384 links, because they
  take the part of the memory that `platform-mem` returns after growing it; the
  benchmarks wrap the memory in [`memory::Whole`](rust/src/memory.rs), which
  returns the whole memory, and
  [`experiments/check-full-size.sh`](experiments/check-full-size.sh) checks
  the largest scheduled size.

## Running locally

Start a disposable Redis server without persistence, then run both suites of
both languages and generate the tables and charts:

```sh
docker run -d --rm --name redis-comparison -p 127.0.0.1:6379:6379 \
  redis:8.10.2 redis-server --save '' --appendonly no
export REDIS_URL=redis://localhost:6379
python3 -m pip install -r scripts/requirements.txt
examples/run-comparison.sh 1000
```

The sizes are set with `BENCHMARK_BACKGROUND_LINKS` (`B`, default 1,000) and
`BENCHMARK_LINKS` (`N`, default 100), and `BENCHMARK_BACKEND=redis` or
`doublets` runs one backend only:

```sh
(cd rust && BENCHMARK_BACKEND=doublets cargo bench --bench bench)
BENCHMARK_BACKEND=redis dotnet run -c Release --project csharp/RedisVSDoublets
```

Tests check that every store gives the same results as a model of the
operations, including Redis in every mode, and that every undo restores the
background data; see [CONTRIBUTING.md](CONTRIBUTING.md) for all
local checks.

## Results

The [Benchmarks workflow](.github/workflows/benchmarks.yml) generates the
content between the markers below on every push to `main`. `main` is
protected, so the workflow needs a `BENCHMARK_PUSH_TOKEN` secret (or
permission for `github-actions` to push) to publish it; until then the results
are in the `Benchmark report` artifact of the run.

<!-- results:start -->
<!-- results:end -->
