#!/bin/bash

source "prelude.sh"
echo I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.

# Testing with relative parent directory (..) as search root
REL_ROOT=".."

COMMAND_FIND="fdf '.' '$REL_ROOT' -HI"
COMMAND_FD="fd '.' '$REL_ROOT' -HI"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-relative-dir-test.md

check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
echo the count of files in the results.fd are $( cat /tmp/results.fd | wc -l)
echo the count of files in the results.find are $( cat /tmp/results.find | wc -l)
echo "The total difference is $(( $(diff /tmp/results.fd /tmp/results.find | wc -l) / 2 ))"
check_missing=$(diff /tmp/results.fd /tmp/results.find | awk '{print $2}' | tr -s ' ')
echo "The missing files are: $check_missing"