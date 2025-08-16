#!/bin/bash

# Benchmark script comparing fdf vs fd performance on full user home (mostly for readme purposes)
SEARCH_ROOT="$HOME"
OUTPUT_FILE="fdf_vs_fd_benchmark.md"

# Clean previous results if exists
rm -f "$OUTPUT_FILE"

echo "Running fdf vs fd benchmarks..."
echo "Search root: $SEARCH_ROOT"
echo ""

PATTERN="'.*[0-9].*(md|\.c)$'"

hyperfine \
  --warmup 3 \
  --export-markdown "$OUTPUT_FILE" \
  --style full \
  "fdf 'hi' '$SEARCH_ROOT' -HI -E 'c' | wc -l" \
  "fd 'hi' '$SEARCH_ROOT' -HI --extension 'c' | wc -l" \
  "fdf $PATTERN '$SEARCH_ROOT' -HI | wc -l" \
  "fd $PATTERN '$SEARCH_ROOT' -HI | wc -l" \
  "fdf '^h' '$SEARCH_ROOT' -HI --extension c | wc -l" \
  "fd '^h' '$SEARCH_ROOT' -HI --extension c | wc -l" \
  "fdf '\.py$' '$SEARCH_ROOT' -HI -t f | wc -l" \
  "fd '\.py$' '$SEARCH_ROOT' -HI -t f | wc -l"

sed -i "1s/^/# fdf vs fd Benchmark Results\n\n/" "$OUTPUT_FILE"
sed -i "4s/^/## Test 1: Simple pattern with extension filter\n/" "$OUTPUT_FILE"
sed -i "12s/^/## Test 2: Regex pattern for numbered files (md/c)\n/" "$OUTPUT_FILE"
sed -i "20s/^/## Test 3: '^h' prefix on C files\n/" "$OUTPUT_FILE"
sed -i "28s/^/## Test 4: All Python files\n/" "$OUTPUT_FILE"

echo ""
echo "Benchmark results saved to $OUTPUT_FILE"