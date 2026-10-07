#!/usr/bin/env bash
set -euo pipefail
language=${1:?language required}
echo "# links: ${BENCHMARK_LINKS:-100}"
echo "# background: ${BENCHMARK_BACKGROUND_LINKS:-1000}"
echo "# samples: ${BENCHMARK_SAMPLES:-10}"
cpu=$(lscpu | sed -n 's/^Model name: *//p' | head -n 1)
echo "# cpu: ${cpu:-unknown}"
echo "# date: $(date -u +%Y-%m-%d)"
echo "# commit: $(git rev-parse HEAD)"
case "$language" in
  rust) echo "# runtime: $(rustc --version)" ;;
  csharp) echo "# runtime: .NET SDK $(dotnet --version)" ;;
  *) exit 1 ;;
esac
if [ -n "${GITHUB_RUN_ID:-}" ]; then
  echo "# run: ${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY}/actions/runs/${GITHUB_RUN_ID}"
fi
