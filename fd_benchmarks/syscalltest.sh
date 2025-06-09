#!/usr/bin/env bash
set -euo pipefail
source "new_prelude.sh"
FDF="fdf"
FD="fd"
DIR="."
STRACE_OUT_DIR="strace_outputs"
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

  STRACE_FDF="$STRACE_OUT_DIR/${CASE_NAME}_fdf.txt"
  STRACE_FD="$STRACE_OUT_DIR/${CASE_NAME}_fd.txt"

  strace -c -o "$STRACE_FDF" "$FDF" "$PATTERN" "$DIR" -HI
  strace -c -o "$STRACE_FD" "$FD" "$PATTERN" "$DIR" -HI

  echo "Finished $CASE_NAME"
  echo
done

echo "🧾 Side-by-side syscall comparisons:"

for CASE_NAME in "${CASE_NAMES[@]}"; do
  echo "=== $CASE_NAME ==="
  diff -y --suppress-common-lines \
    "$STRACE_OUT_DIR/${CASE_NAME}_fdf.txt" \
    "$STRACE_OUT_DIR/${CASE_NAME}_fd.txt" || true
  echo
done
