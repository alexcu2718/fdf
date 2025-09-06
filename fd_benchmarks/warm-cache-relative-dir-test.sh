#!/usr/bin/env bash

set -euo pipefail
source "prelude.sh"
source "new_prelude.sh"
#i dont use gitignore so -HI is equivalent on both tools


OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

REL_ROOT=".."

COMMAND_FIND="fdf '.' '$REL_ROOT' -HI"
COMMAND_FD="fd '.' '$REL_ROOT' -HI"

echo -e "\nGetting accurate file counts..."

echo 'I HAVE TO EXPLICITLY FILTER SYSTEMD FILES WHICH ARE TEMPORARY FALSE RESULTS'
fd_count=$(eval "$COMMAND_FD" | grep -vc 'systemd-private' ) #thanks shellcheck!
fdf_count=$(eval "$COMMAND_FIND" | grep -vc 'systemd-private' )
echo "fd count: $fd_count"
echo "fdf count: $fdf_count"

echo -e "\nRunning relative directory benchmarks..."
hyperfine \
  --warmup 3 \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FIND" \
  "$COMMAND_FD" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-relative-dir-test.md" 


eval "$COMMAND_FD" | grep -v 'systemd-private' | sort > "$OUTPUT_DIR/fd_relative.lst"

eval "$COMMAND_FIND" | grep -v 'systemd-private' | sort > "$OUTPUT_DIR/fdf_relative.lst"

diff -u --color "$OUTPUT_DIR/fd_relative.lst" "$OUTPUT_DIR/fdf_relative.lst" > "$OUTPUT_DIR/fd_diff_relative.md"

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

else
  echo "No differences found in direct execution"
fi

