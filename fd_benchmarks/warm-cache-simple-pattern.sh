#!/bin/bash

source "prelude.sh"
source "new_prelude.sh"

OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

# Pattern definition
pattern="'.*[0-9].*(md|\.c)$'"

# Command definitions
COMMAND_FIND="fdf -HI $pattern '$SEARCH_ROOT'"
COMMAND_FD="fd -HI $pattern '$SEARCH_ROOT'"

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
  --export-markdown "$OUTPUT_DIR/results-warm-cache-simple-pattern.md"

# Improved difference checking
echo -e "\nAnalysing differences..."
eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_pattern.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_pattern.lst"

# Create the diff file
diff -u "$OUTPUT_DIR/fd_pattern.lst" "$OUTPUT_DIR/fdf_pattern.lst" > "./fd_diff_simple_pattern.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_pattern.lst" "$OUTPUT_DIR/fdf_pattern.lst" | wc -l)
echo "Total lines differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_pattern.lst" "$OUTPUT_DIR/fdf_pattern.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_pattern.lst" "$OUTPUT_DIR/fdf_pattern.lst"
  
  echo -e "\nNote: Small differences may occur due to:"
  echo "- Filesystem timestamp changes during execution"
  echo "- Race conditions in very fast scans"
  echo "- Hyperfine measurement artifacts"
else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-simple-pattern.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_simple_pattern.md"
