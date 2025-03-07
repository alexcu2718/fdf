#!/bin/bash

source "prelude.sh"
echo I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.


COMMAND_FIND="fdf  'xonsh' '$SEARCH_ROOT' -HI"
#COMMAND_FIND="find '$SEARCH_ROOT'"
COMMAND_FD="fd  'xonsh' '$SEARCH_ROOT' -HI"
#COMMAND_FD="fd --hidden --no-ignore '' '$SEARCH_ROOT'"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-no-pattern_test.md

check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
echo the count of files in the results.fd are $( cat /tmp/results.fd | wc -l)
echo the count of files in the results.find are $( cat /tmp/results.find | wc -l)
echo the total difference are $( diff /tmp/results.fd /tmp/results.find | wc -l)

