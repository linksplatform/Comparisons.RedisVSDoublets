# Comparisons.RedisVSDoublets

Compare Redis and Doublets in Rust and C# using the same link operations and sizes.
Rust uses `redis` and `doublets`; C# uses StackExchange.Redis and Platform.Data.Doublets.
Doublets uses the united store in heap memory, without files, as required by [issue #1](https://github.com/linksplatform/Comparisons.RedisVSDoublets/issues/1).
Redis is a local server with snapshotting and AOF disabled. Connections and background setup are outside the timed work.

A link is `(id, source, target)`. Every store starts with `B` point links `(id, id, id)`.
An iteration performs one of these operations:

| Operation | Work per iteration |
| --- | --- |
| Create | Create `N` point links; undo by deleting them in descending order |
| Update | Change the first `N` links to `(id, id, id % B + 1)`; undo to points |
| Delete | Delete the last `N` points in descending order; undo by recreating them |
| Each All | Enumerate all `B` links once |
| Each Identity | Query `[id, *, *]` for the first `N` ids |
| Each Concrete | Query `[*, id, id]` for the first `N` ids |
| Each Outgoing | Query `[*, id, *]` for the first `N` ids |
| Each Incoming | Query `[*, *, id]` for the first `N` ids |

Both runners use three warm-up iterations, then report the median and sample standard deviation of the measured iterations in nanoseconds.
Undo runs after every iteration and is excluded from timing. Enumeration consumes every returned link.
PRs use `B = 1,000`, `N = 100`, 10 samples; main uses `B = 10,000 / 100,000 / 1,000,000`, `N = 1,000`, 30 samples.
The times describe an entire iteration, rather than one link. Rust and C# use the same bounded sampling procedure; comparisons are within each language.

Redis represents links in a hash and maintains source, target and pair set indexes. Shared Lua scripts perform each operation atomically in one request, through each language's native client.
Each store owns a unique key prefix and deletes only its own keys on disposal; there is no `FLUSHDB`.
The benchmark uses a single Redis server. Redis Cluster, duplicate endpoint pairs, cascading deletes and arbitrary reuse of deleted non-tail ids are outside this workload.
The tests also cover asymmetric links, multiple matches, stale-index removal, missing ids and isolated stores.

Redis timings include client serialization, a local network round trip and server execution. Doublets executes in the runner process.
These are measurements of those access paths and data representations. They do not measure persistence, remote networks or concurrent clients.
The largest size leaves room below the known `doublets` 0.5.0 capacity limit of 1,040,383 links.

## Running locally

Install the pinned Rust toolchain, .NET 10 SDK, Python 3.12+, and Docker. Start the same Redis as CI:

```bash
docker run -d --name redis-comparison -p 127.0.0.1:6379:6379 \
  redis:7.4.2 redis-server --save '' --appendonly no
export REDIS_URL=redis://127.0.0.1:6379
python3 -m pip install -r scripts/requirements.txt
cargo fmt --manifest-path rust/Cargo.toml --check
cargo clippy --locked --release --all-targets --manifest-path rust/Cargo.toml -- -D warnings
cargo test --locked --release --manifest-path rust/Cargo.toml -- --include-ignored
(cd csharp && dotnet restore --locked-mode && dotnet format --no-restore --verify-no-changes \
  && dotnet build -c Release --no-restore -warnaserror \
  && dotnet test -c Release --no-build --no-restore -- --timeout 2m)
ruff check scripts
ruff format --check scripts
python3 -m unittest discover -s scripts -v
./examples/run-comparison.sh
```

`REDIS_URL` is optional for Rust's ignored integration test (include it with `--include-ignored`); C#'s Redis test is skipped when the variable is absent. CI sets it and runs both integration tests.
To change sizes, set `BENCHMARK_BACKGROUND_LINKS`, `BENCHMARK_LINKS`, and `BENCHMARK_SAMPLES` before running the example. Use an empty `results/` directory when switching sizes.
Require `0 < N <= B`, `B + N <= 1,040,383` and at least three samples.

The workflow has independent test jobs and a language × backend × size benchmark matrix.
A final job validates all results and generates tables and linear/log charts. It uploads the report on PRs and commits results on main.
The renderer rejects incomplete backend/language pairs, different sizes or sample counts, duplicate/error results and missing provenance.
Dependencies are pinned with Cargo and NuGet lockfiles; no packages are published.

## Results

Generated content stays between the markers below. Every Doublets cell compares its median to Redis: `× faster`, `× slower`, or `≈ same` if the medians differ by less than 5% or their ±1 standard deviation ranges overlap.
Provenance includes server/library/runtime versions, CPU, commit, date and the CI run link (or a local run).

<!-- results:start -->
### Rust

_No results yet._

### C#

_No results yet._
<!-- results:end -->
