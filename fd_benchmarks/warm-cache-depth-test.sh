#!/bin/bash

source "prelude.sh"
source "new_prelude.sh"

echo "I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET."
echo "Note: Hyperfine may show small discrepancies due to benchmarking overhead."

# Setup output directories
OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

# Command definitions with depth limit
DEPTH_LIMIT=2
COMMAND_FIND="fdf '.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT"
COMMAND_FD="fd '.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT"

# First get accurate baseline counts
echo -e "\nGetting accurate file counts..."
fd_count=$(eval "$COMMAND_FD" | wc -l)
fdf_count=$(eval "$COMMAND_FIND" | wc -l)
echo "fd count: $fd_count"
echo "fdf count: $fdf_count"

# Run benchmarks with stabilization
echo -e "\nRunning depth-limited benchmarks (depth=$DEPTH_LIMIT)..."
hyperfine \
  --warmup "$WARMUP_COUNT" \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FIND" \
  "$COMMAND_FD" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-depth-test.md"

# Improved difference checking
echo -e "\nAnalyzing differences..."
eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_depth.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_depth.lst"

# Create the diff file
diff -u "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst" > "./fd_diff_depth.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst" | wc -l)
echo "Total files found by fd: $(wc -l < "$OUTPUT_DIR/fd_depth.lst")"
echo "Total files found by fdf: $(wc -l < "$OUTPUT_DIR/fdf_depth.lst")"
echo "Total files differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst"
  
  echo -e "\nNote: Differences may occur due to:"
  echo "- Different depth calculation methods between tools"
  echo "- Filesystem timestamp changes during execution"
  echo "- Race conditions in fast scans"
  echo "- Hyperfine measurement artifacts"
else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-depth-test.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_depth.md"
