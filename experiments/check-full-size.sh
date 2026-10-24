#!/usr/bin/env bash
# Probes the largest scheduled size (1,000,000 background links, 1,000 links
# per iteration) of the in-memory Doublets stores with bounded memory, without
# Redis. Only the groups that create links run (Create of the links benchmarks,
# KeyValue_Set and Queue_Push of the usage benchmarks), with the usual sampling,
# so every store grows past the 1,040,384 links at which doublets 0.5.0 to 0.8.0 fail
# without rust/src/memory.rs.
# Usage: experiments/check-full-size.sh > experiments/logs/check-full-size.log
set -euo pipefail
cd "$(dirname "$0")/.."
export BENCHMARK_BACKEND=doublets
export BENCHMARK_BACKGROUND_LINKS=1000000
export BENCHMARK_LINKS=1000
(cd rust && cargo bench --locked --bench bench --no-run)
dotnet build csharp/RedisVSDoublets -c Release -warnaserror
for group in Create/ KeyValue_Set/ Queue_Push/; do
  # 4 GiB of address space for Rust; the build is outside the limit.
  (ulimit -v 4194304; cd rust && CRITERION_HOME="$(mktemp -d)" \
    cargo bench --locked --bench bench -- --output-format bencher --noplot "$group")
  # The managed heap of .NET is limited to 1 GiB.
  DOTNET_GCHeapHardLimit=0x40000000 dotnet run -c Release --no-build --project csharp/RedisVSDoublets -- "$group"
done
