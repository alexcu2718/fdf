#!/usr/bin/env bash


source "prelude.sh"
source "new_prelude.sh"

#i dont use gitignore so -HI is equivalent on both tools

OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

COMMAND_FIND="fdf '.' '$SEARCH_ROOT' -HI --type e"
COMMAND_FD="fd '.' '$SEARCH_ROOT' -HI --type e"

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
  --export-markdown "$OUTPUT_DIR/results-warm-cache-type-filtering-empty.md"

echo -e "\nAnalyzing differences..."
eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_type_e.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_type_e.lst"

diff -u "$OUTPUT_DIR/fd_type_e.lst" "$OUTPUT_DIR/fdf_type_e.lst" > "$OUTPUT_DIR/fd_diff_type_e.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_type_e.lst" "$OUTPUT_DIR/fdf_type_e.lst" | wc -l)
echo "Total files differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_type_e.lst" "$OUTPUT_DIR/fdf_type_e.lst"

  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_type_e.lst" "$OUTPUT_DIR/fdf_type_e.lst"
else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-type-filtering-empty.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_type_e.md"
