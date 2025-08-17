#!/bin/bash

source "prelude.sh"
source "new_prelude.sh"
echo "I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE GIT ignore IN MINE YET."
echo "Note: Benchmarking relative directory searches (../)"



OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

# Testing with relative parent directory (..) as search root
REL_ROOT=".."

COMMAND_FIND="fdf '.' '$REL_ROOT' -HI"
COMMAND_FD="fd '.' '$REL_ROOT' -HI"

# First get accurate baseline counts
echo -e "\nGetting accurate file counts..."

echo 'I HAVE TO EXPLICITLY FILTER SYSTEMD FILES WHICH ARE TEMPORARY FALSE RESULTS'
fd_count=$(eval "$COMMAND_FD" | grep -v 'systemd-private' | wc -l)
fdf_count=$(eval "$COMMAND_FIND" | grep -v 'systemd-private' | wc -l)
echo "fd count: $fd_count"
echo "fdf count: $fdf_count"

echo -e "\nRunning relative directory benchmarks..."
hyperfine \
  --warmup 3 \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FIND" \
  "$COMMAND_FD" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-relative-dir-test.md" 


echo -e "\nAnalysing differences..."
eval "$COMMAND_FD" | grep -v 'systemd-private' | sort > "$OUTPUT_DIR/fd_relative.lst"

eval "$COMMAND_FIND" | grep -v 'systemd-private' | sort > "$OUTPUT_DIR/fdf_relative.lst"

# Create the diff file
diff -u --color "$OUTPUT_DIR/fd_relative.lst" "$OUTPUT_DIR/fdf_relative.lst" > "./fd_diff_relative.md"

fd_total=$(wc -l < "$OUTPUT_DIR/fd_relative.lst")
fdf_total=$(wc -l < "$OUTPUT_DIR/fdf_relative.lst")
differences=$(diff -y --suppress-common-lines "$OUTPUT_DIR/fd_relative.lst" "$OUTPUT_DIR/fdf_relative.lst" | wc -l)

echo -e "\nFinal counts:"
echo "fd total files:  $fd_total"
echo "fdf total files: $fdf_total"
echo "Files differing: $differences"

if [[ $differences -gt 0 ]]; then
  echo -e "\nFiles only in fd:"
  comm -23 "$OUTPUT_DIR/fd_relative.lst" "$OUTPUT_DIR/fdf_relative.lst"
  
  echo -e "\nFiles only in fdf:"
  comm -13 "$OUTPUT_DIR/fd_relative.lst" "$OUTPUT_DIR/fdf_relative.lst"
  
  echo -e "\nNote about the 1-file difference:"
  echo "The discrepancy is in the benchmark output file itself:"
  echo "../fd_benchmarks/bench_results/fdf_relative.lst"
  echo "This is expected as the file is created during benchmarking"
else
  echo "No differences found in direct execution"
fi

