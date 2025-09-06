#!/usr/bin/env bash

source "prelude.sh"
source "new_prelude.sh"
#i dont use gitignore so -HI is equivalent on both tools
COMMAND_FIND="fdf '.' '$HOME' -HI"
COMMAND_FD="fd '.' '$HOME' -HI"

OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

echo -e "\nGetting accurate file counts..."
fd_count=$(eval "$COMMAND_FD"  | grep -vcE 'paru/clone/.*/pkg' ) #filter stupid package manager files
fdf_count=$(eval "$COMMAND_FIND" | grep -vcE 'paru/clone/.*/pkg')
echo "fd count: $fd_count"
echo "fdf count: $fdf_count"

echo -e "\nRunning benchmarks..."
hyperfine \
  --warmup "$WARMUP_COUNT" \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FIND" \
  "$COMMAND_FD" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-no-pattern_home_dir.md"

eval "$COMMAND_FD" | grep -vE 'paru/clone/.*/pkg' | sort > "$OUTPUT_DIR/fd_home_dir.lst"
eval "$COMMAND_FIND" | grep -vE 'paru/clone/.*/pkg' | sort > "$OUTPUT_DIR/fdf_home_dir.lst"

diff -u "$OUTPUT_DIR/fd_home_dir.lst" "$OUTPUT_DIR/fdf_home_dir.lst" > "$OUTPUT_DIR/fd_diff_no_pattern_home_dir.md"

differences=$(comm -3 "$OUTPUT_DIR/fd_home_dir.lst" "$OUTPUT_DIR/fdf_home_dir.lst" | wc -l)
echo "Total lines differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_home_dir.lst" "$OUTPUT_DIR/fdf_home_dir.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_home_dir.lst" "$OUTPUT_DIR/fdf_home_dir.lst"
  

else
  echo "No differences found in direct execution"
fi

echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-no-pattern_home_dir.md"
echo "Diff results saved to $OUTPUT_DIR/fd_diff_no_pattern_home_dir.md"
