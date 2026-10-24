# Contributing

Use a pull request and keep changes focused on the benchmark comparison. CI
checks formatting before tests, uses fixed runner, toolchain and Redis versions,
and pins every action to a commit. Documentation, workflow and Python changes
have their own validation workflow (`CI quality`).

## Local checks

Use the SDK from `csharp/global.json`, the Rust toolchain from
`rust-toolchain.toml`, Python 3.15 or later, Docker for Redis and actionlint,
and Node.js for the Markdown and secret checks.

The tests empty the Redis database that `REDIS_URL` points to (`FLUSHDB`), so
start a disposable Redis server for them, and never run the tests and the
benchmarks against the same server at the same time:

```sh
docker run -d --rm --name redis-tests -p 6380:6379 \
  redis:8.10.2 redis-server --save '' --appendonly no
export REDIS_URL=redis://localhost:6380
```

Create a virtual environment before installing the Python tools:

```sh
python -m venv .venv
. .venv/bin/activate
python -m pip install -r scripts/requirements.txt -r scripts/requirements-ci.txt
ruff check scripts
ruff format --check scripts
mypy scripts
bandit -r scripts -q
python -m unittest discover -s scripts -v
(cd rust && cargo fmt --check)
(cd rust && cargo clippy --locked --release --all-targets -- -D warnings)
(cd rust && cargo test --locked --release -- --include-ignored)
(cd rust && cargo bench --locked --no-run)
(cd csharp && dotnet format RedisVSDoublets.slnx --verify-no-changes)
(cd csharp && dotnet build RedisVSDoublets.slnx -c Release -warnaserror)
(cd csharp && dotnet test --project RedisVSDoublets.Tests -c Release \
  --no-build -- --timeout 15m)
npx --yes markdownlint-cli@0.49.1 README.md README.ru.md CONTRIBUTING.md \
  experiments/README.md
git ls-files -z -- ':!*.png' | xargs -0 npx --yes -p secretlint@13.0.7 \
  -p @secretlint/secretlint-rule-preset-recommend@13.0.7 secretlint
```

The language workflows also format, lint and build every Rust and C#
experiment (`experiments/rust-*` and `experiments/csharp-*`).

Workflow validation runs actionlint with ShellCheck and two zizmor passes. Run
the pinned actionlint container and both zizmor commands from
`.github/workflows/quality.yml` locally when changing automation. Optional
pre-commit hooks (ruff, rustfmt and `dotnet format whitespace`) are configured
in `.pre-commit-config.yaml`:

```sh
python -m pip install pre-commit
pre-commit install
```

## Benchmarks

`examples/run-comparison.sh` runs both benchmark suites of both languages like
the `Benchmarks` workflow does, against the disposable Redis server of
`REDIS_URL`, and generates the README tables and the charts from the results.
Every benchmark checks its
results outside the measured time: the store must hold exactly the background
data before the first and after the last iteration, and one iteration before
the measurement must find exactly the expected links or values. A benchmark
that measures wrong work fails instead of reporting a time.

The `Benchmarks` workflow runs on pull requests and uploads the generated
README and charts as the `Benchmark report` artifact; on `main` it commits them.
`main` is protected, so the publication needs a `BENCHMARK_PUSH_TOKEN` secret
(or permission for `github-actions` to push); without it the publication step
fails with an explicit error that says what to do.

## Dependencies

Dependabot updates the NuGet, Cargo, Python and GitHub Actions dependencies of
the benchmarks and of the experiments after a cooldown of seven days, and
`AutoMerge.yml` merges its patch and minor updates once CI passes. The
`Dependencies` workflow audits Python, every committed Rust lockfile and the
direct and transitive NuGet packages weekly and on relevant changes. Run
`pip-audit -r scripts/requirements.txt -r scripts/requirements-ci.txt`,
`cargo audit --file <Cargo.lock> --deny warnings` and
`python scripts/audit_dotnet.py` locally with the workflow tool versions.

This repository contains benchmark executables, without a package release or
container deployment pipeline.
