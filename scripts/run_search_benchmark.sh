#!/bin/sh
set -eu

report_path="${1:-artifacts/search-benchmark/report.json}"
mkdir -p "$(dirname "$report_path")"

CLIPMEM_SEARCH_BENCH_REPORT="$report_path" \
  cargo test --test search_benchmark benchmark_search_quality_and_latency \
    -- --ignored --exact --nocapture

test -s "$report_path"
printf 'search benchmark report: %s\n' "$report_path"
