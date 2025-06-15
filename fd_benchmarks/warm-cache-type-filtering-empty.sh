#!/bin/bash

source "prelude.sh"
source "new_prelude.sh"

echo "I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET."
echo "Note: Hyperfine may show small discrepancies due to benchmarking overhead."

# Setup output directories
OUTPUT_DIR="($dirname $(which fdf))/bench_results"
mkdir -p "$OUTPUT_DIR"

# Command definitions
COMMAND_FIND="fdf '.' '$SEARCH_ROOT' -HI --type e"
COMMAND_FD="fd '.' '$SEARCH_ROOT' -HI --type e"

# First get accurate baseline counts
echo -e "\nGetting accurate file counts..."
fd_count=$(eval "$COMMAND_FD" | wc -l)
fdf_count=$(eval "$COMMAND_FIND" | wc -l)
echo "fd count: $fd_count"
echo "fdf count: $fdf_count"

# Run benchmarks with stabilization
echo -e "\nRunning benchmarks..."
hyperfine \
  --warmup "$WARMUP_COUNT" \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FIND" \
  "$COMMAND_FD" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-type-filtering-empty.md"

