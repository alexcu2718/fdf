#!/bin/bash
set -euo pipefail
source "new_prelude.sh"
FDF="fdf"
FD="fd"
DIR="$HOME"
STRACE_OUT_DIR="./bench_results"
mkdir -p "$STRACE_OUT_DIR"

declare -a PATTERNS=(
  "NOMATCHLOL"
  "Cargo\\.toml"
  "\\.rs$"
)
declare -a CASE_NAMES=(
  "Case1"
  "Case2"
  "Case3"
)

for i in "${!PATTERNS[@]}"; do
  PATTERN="${PATTERNS[i]}"
  CASE_NAME="${CASE_NAMES[i]}"

  echo "Running $CASE_NAME (pattern: $PATTERN)"

  STRACE_FDF="$STRACE_OUT_DIR/strace_outputs_${CASE_NAME}_fdf.txt"
  STRACE_FD="$STRACE_OUT_DIR/strace_outputs_${CASE_NAME}_fd.txt"
  echo "testing syscalls on directory $DIR USING $PATTERN"
  strace -fc -o "$STRACE_FDF" "$FDF" "$PATTERN" "$DIR" -HI > /dev/null 2>&1
  strace -fc -o "$STRACE_FD" "$FD" "$PATTERN" "$DIR" -HI > /dev/null 2>&1
  echo "Finished $CASE_NAME"
done

echo "Side-by-side syscall comparisons:"


for CASE_NAME in "${CASE_NAMES[@]}"; do
  echo "=== $CASE_NAME ==="
  diff -y --suppress-common-lines \
    "$STRACE_OUT_DIR/strace_outputs_${CASE_NAME}_fdf.txt" \
    "$STRACE_OUT_DIR/strace_outputs_${CASE_NAME}_fd.txt" || true
  echo
done
