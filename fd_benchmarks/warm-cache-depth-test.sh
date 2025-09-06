#!/usr/bin/env bash


source "prelude.sh"
source "new_prelude.sh"

#i dont use gitignore so -HI is equivalent on both tools
OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

DEPTH_LIMIT=2
COMMAND_FIND="fdf '.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT"
COMMAND_FD="fd '.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT"

# First get accurate baseline counts
echo -e "\nGetting accurate file counts..."
fd_count=$(eval "$COMMAND_FD" | wc -l)
fdf_count=$(eval "$COMMAND_FIND" | wc -l)
echo "fd count: $fd_count"
echo "fdf count: $fdf_count"

echo -e "\nRunning depth-limited benchmarks (depth=$DEPTH_LIMIT)..."
hyperfine \
  --warmup "$WARMUP_COUNT" \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FIND" \
  "$COMMAND_FD" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-depth-test.md"

eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_depth.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_depth.lst"

diff -u "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst" > "$OUTPUT_DIR/fd_diff_depth.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst" | wc -l)
echo "Total files found by fd: $(wc -l < "$OUTPUT_DIR/fd_depth.lst")"
echo "Total files found by fdf: $(wc -l < "$OUTPUT_DIR/fdf_depth.lst")"
echo "Total files differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_depth.lst" "$OUTPUT_DIR/fdf_depth.lst"
  

else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-depth-test.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_depth.md"
