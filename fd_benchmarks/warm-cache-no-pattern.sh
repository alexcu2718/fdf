#!/bin/bash

source "prelude.sh"
echo I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.


COMMAND_FIND="fdf  '.' '$SEARCH_ROOT' -HI"
#COMMAND_FIND="find '$SEARCH_ROOT'"
COMMAND_FD="fd  '.' '$SEARCH_ROOT' -HI"
#COMMAND_FD="fd --hidden --no-ignore '' '$SEARCH_ROOT'"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-no-pattern.md

check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
echo the count of files in the results.fd are $( cat /tmp/results.fd | wc -l)
echo the count of files in the results.find are $( cat /tmp/results.find | wc -l)
total_diff=$(diff /tmp/results.fd /tmp/results.find | wc -l)
echo "The total difference is $(($total_diff / 2))"
check_missing=$(diff /tmp/results.fd /tmp/results.find | awk '{print $2}' | tr -s ' ')
echo "The missing files are: $check_missing"
echo "however, when searching directly for $check_missing, we find that they are not missing."

