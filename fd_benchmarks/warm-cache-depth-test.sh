#!/bin/bash


source "prelude.sh"
echo I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.
source "new_prelude.sh"

COMMAND_FIND="fdf '.' '$SEARCH_ROOT' -HI -d 2"
COMMAND_FD="fd '.' '$SEARCH_ROOT' -HI -d 2"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-depth-test.md

check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
echo the count of files in the results.fd are $( cat /tmp/results.fd | wc -l)
echo the count of files in the results.find are $( cat /tmp/results.find | wc -l)
echo "The total difference is $(( $(diff /tmp/results.fd /tmp/results.find | wc -l) / 2 ))"
check_missing=$(diff /tmp/results.fd /tmp/results.find | awk '{print $2}' | tr -s ' ')
echo "The missing files are: $check_missing"
