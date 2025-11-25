#!/usr/bin/env bash


source "prelude.sh"
source "new_prelude.sh"
PATTERN="THISSHOULDNEVERMATCH"
#i dont use gitignore so -HI is equivalent on both tools
COMMAND_FD="fd $PATTERN '$HOME' -HI"
COMMAND_FIND="fdf $PATTERN '$HOME' -HI"
OUTPUT_DIR="./bench_results"
mkdir -p "$OUTPUT_DIR"

echo -e "\nRunning benchmarks..."
hyperfine \
  --warmup "$WARMUP_COUNT" \
  --prepare 'sync; sleep 0.2' \
  "$COMMAND_FD" \
  "$COMMAND_FIND" \
  --export-markdown "$OUTPUT_DIR/results-warm-cache-no-match-home.md"
