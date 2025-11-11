#!/usr/bin/env bash


source "prelude.sh"
source "new_prelude.sh"
set -euo pipefail
OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"
# i don't use gitignore so -HI is equivalent on both tools

echo "running size filtering test for $HOME  , these can take up to 2mins in worst case due to benchmarking multiple runs"
COMMAND_FIND="fdf -HI --size +1mb '' '$HOME'"
COMMAND_FD="fd -HI --size +1mb '' '$HOME'"

echo -e "\nGetting accurate file counts..."
fd_count=$(eval "$COMMAND_FD" | wc -l)
fdf_count=$(eval "$COMMAND_FIND" | wc -l)
echo "fd count: $fd_count"
echo "fdf count: $fdf_count"

echo -e "\nRunning benchmarks..."
hyperfine \
  --warmup "$WARMUP_COUNT" \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FIND" \
  "$COMMAND_FD" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-size-home.md"


eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_size_home.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_size_home.lst"

diff -u "$OUTPUT_DIR/fd_size_home.lst" "$OUTPUT_DIR/fdf_size_home.lst" > "$OUTPUT_DIR/fd_diff_size_home.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_size_home.lst" "$OUTPUT_DIR/fdf_size_home.lst" | wc -l)
echo "Total files differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_size_home.lst" "$OUTPUT_DIR/fdf_size_home.lst"

  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_size_home.lst" "$OUTPUT_DIR/fdf_size_home.lst"

else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-size-home.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_size_home.md"
