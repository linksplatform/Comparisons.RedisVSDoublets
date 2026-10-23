#!/usr/bin/env bash
# Runs the benchmarks like .github/workflows/benchmarks.yml does, writing
# results/<language>-<background>-<backend>.txt for scripts/benchmark_report.py.
# Usage: experiments/run_local_benchmarks.sh [background links...]
# (default 1000; BENCHMARK_LINKS defaults to 100). REDIS_URL must point to an
# empty, disposable Redis database: the benchmarks run FLUSHDB.
set -euo pipefail
cd "$(dirname "$0")/.."
export BENCHMARK_LINKS="${BENCHMARK_LINKS:-100}"
export REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
mkdir -p results
(cd rust && cargo bench --locked --bench bench --no-run)
dotnet build csharp/RedisVSDoublets -c Release -warnaserror
for background in "${@:-1000}"; do
  for backend in redis doublets; do
    export BENCHMARK_BACKGROUND_LINKS="$background" BENCHMARK_BACKEND="$backend"
    {
      python3 scripts/benchmark_header.py "$backend"
      (cd rust && CRITERION_HOME="$(mktemp -d)" cargo bench --locked --bench bench -- --output-format bencher --noplot)
    } | tee "results/rust-$background-$backend.txt"
    {
      python3 scripts/benchmark_header.py "$backend"
      dotnet run -c Release --no-build --project csharp/RedisVSDoublets
    } | tee "results/csharp-$background-$backend.txt"
  done
done
