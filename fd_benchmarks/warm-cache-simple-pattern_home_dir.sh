#!/usr/bin/env bash


source "prelude.sh"
source "new_prelude.sh"

OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

pattern="'.*[0-9].*(md|\.c)$'"
COMMAND_FIND="fdf -HI $pattern '$HOME'"
COMMAND_FD="fd -HI $pattern '$HOME'"

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
  --export-markdown "$OUTPUT_DIR/results-warm-cache-simple-pattern_home_dir.md"

echo -e "\nAnalysing differences..."
eval "$COMMAND_FD" | sort > "$OUTPUT_DIR/fd_pattern_home_dir.lst"
eval "$COMMAND_FIND" | sort > "$OUTPUT_DIR/fdf_pattern_home_dir.lst"

diff -u "$OUTPUT_DIR/fd_pattern_home_dir.lst" "$OUTPUT_DIR/fdf_pattern_home_dir.lst" > "$OUTPUT_DIR/fd_diff_simple_pattern_home_dir.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_pattern_home_dir.lst" "$OUTPUT_DIR/fdf_pattern_home_dir.lst" | wc -l)
echo "Total lines differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_pattern_home_dir.lst" "$OUTPUT_DIR/fdf_pattern_home_dir.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_pattern_home_dir.lst" "$OUTPUT_DIR/fdf_pattern_home_dir.lst"
  

else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-simple-pattern_home_dir.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_simple_pattern_home_dir.md"
