#!/usr/bin/env bash
# Bound a capacity probe at the largest scheduled size (RAM only, no Redis).
set -euo pipefail
cd "$(dirname "$0")/.."
export BENCHMARK_BACKEND=doublets
export BENCHMARK_BACKGROUND_LINKS=1000000
export BENCHMARK_LINKS=1000
export BENCHMARK_SAMPLES=3
(ulimit -v 2097152; ./rust/target/release/redis-vs-doublets)
# The managed heap is limited to 256 MiB; the probe creates a finite 1,001,000 links.
DOTNET_GCHeapHardLimit=0x10000000 dotnet csharp/RedisVSDoublets/bin/Release/net10.0/RedisVSDoublets.dll
