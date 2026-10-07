#!/usr/bin/env bash
# Reproduce the four quick benchmark jobs locally against a running Redis.
set -euo pipefail
cd "$(dirname "$0")/.."
export REDIS_URL=${REDIS_URL:-redis://127.0.0.1:6379}
export BENCHMARK_BACKGROUND_LINKS=${BENCHMARK_BACKGROUND_LINKS:-1000}
export BENCHMARK_LINKS=${BENCHMARK_LINKS:-100}
export BENCHMARK_SAMPLES=${BENCHMARK_SAMPLES:-10}
mkdir -p results
cargo build --locked --release --manifest-path rust/Cargo.toml
(cd csharp && dotnet restore --locked-mode && dotnet build -c Release --no-restore)
for language in rust csharp; do
  for backend in redis doublets; do
    export BENCHMARK_BACKEND=$backend
    (
      cd "$language"
      ../scripts/benchmark_header.sh "$language"
      if [ "$language" = rust ]; then
        ./target/release/redis-vs-doublets
      else
        dotnet run -c Release --no-build --project RedisVSDoublets
      fi
    ) | tee "results/$language-$BENCHMARK_BACKGROUND_LINKS-$backend.txt"
  done
done
python3 scripts/benchmark_report.py results --readme README.md --charts docs
