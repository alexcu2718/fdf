#!/bin/bash

source "prelude.sh"
source "new_prelude.sh"

echo "i dont use gitignore so -HI is equivalent on both tools"


OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

# Command definitions
COMMAND_FIND="fdf '.' '$SEARCH_ROOT' -HI --type x"
COMMAND_FD="fd '.' '$SEARCH_ROOT' -HI --type x"

# First get accurate baseline counts
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
  --export-markdown "$OUTPUT_DIR/results-warm-cache-type-filtering-executable.md"

echo -e "\nAnalySing differences..."
eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_type_x.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_type_x.lst"

diff -u "$OUTPUT_DIR/fd_type_x.lst" "$OUTPUT_DIR/fdf_type_x.lst" > "$OUTPUT_DIR/fd_diff_type_x.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_type_x.lst" "$OUTPUT_DIR/fdf_type_x.lst" | wc -l)
echo "Total files differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_type_x.lst" "$OUTPUT_DIR/fdf_type_x.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_type_x.lst" "$OUTPUT_DIR/fdf_type_x.lst"

else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-type-filtering-executable.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_type_x.md"


