#!/usr/bin/env bash


set -euo pipefail
# TODO WRITE THIS  MESS INTO SOME FUNCTIONS
source "prelude.sh"
source "new_prelude.sh"
ask_for_sudo
OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

pattern="'.*[0-9]\.jpg$'"

COMMAND_FD="fd -HI $pattern '$SEARCH_ROOT'"
COMMAND_FIND="fdf -HI $pattern '$SEARCH_ROOT'"

echo -e "\nRunning cold cache benchmarks..."
hyperfine \
    --min-runs 3 \
    --prepare "$RESET_CACHES" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown "$OUTPUT_DIR/results-cold-cache-simple-pattern.md"

echo -e "\nAnalysing differences..."
eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_cold_pattern.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_cold_pattern.lst"

diff -u "$OUTPUT_DIR/fd_cold_pattern.lst" "$OUTPUT_DIR/fdf_cold_pattern.lst" > "$OUTPUT_DIR/fd_diff_cold_pattern.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_cold_pattern.lst" "$OUTPUT_DIR/fdf_cold_pattern.lst" | wc -l)
echo "Total lines differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_cold_pattern.lst" "$OUTPUT_DIR/fdf_cold_pattern.lst"

  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_cold_pattern.lst" "$OUTPUT_DIR/fdf_cold_pattern.lst"

else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-cold-cache-simple-pattern.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_cold_pattern.md"
