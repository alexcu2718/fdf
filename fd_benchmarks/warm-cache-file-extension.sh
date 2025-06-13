#!/bin/bash

source "prelude.sh"
source "new_prelude.sh"

# Setup output directories
OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

# Extension definition
EXT="c"

# Command definitions
COMMAND_FIND="fdf -HI --extension '$EXT' '' '$SEARCH_ROOT'"
COMMAND_FD="fd -HI --extension '$EXT' '' '$SEARCH_ROOT'"

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
  --export-markdown "$OUTPUT_DIR/results-warm-cache-file-extension.md"

# Improved difference checking
echo -e "\nAnalyzing differences..."
eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_extension.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_extension.lst"

# Create the diff file
diff -u "$OUTPUT_DIR/fd_extension.lst" "$OUTPUT_DIR/fdf_extension.lst" > "./fd_diff_extension.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_extension.lst" "$OUTPUT_DIR/fdf_extension.lst" | wc -l)
echo "Total files differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_extension.lst" "$OUTPUT_DIR/fdf_extension.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_extension.lst" "$OUTPUT_DIR/fdf_extension.lst"
  
  echo -e "\nNote about the known edge case:"
  echo " i'm gonna fix this ideally weekend "
  echo "This accounts for the expected 1-file difference"
else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-file-extension.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_extension.md"
