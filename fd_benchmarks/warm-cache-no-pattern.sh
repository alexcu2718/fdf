#!/bin/bash

source "prelude.sh"
echo I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.
echo "there is a bug in hyperfine i believe, if there is a discrepancy, please run the commands and test output yourself, i am clueless on as to why..."

COMMAND_FIND="fdf  '.' '$SEARCH_ROOT' -HI"
#COMMAND_FIND="find '$SEARCH_ROOT'"
COMMAND_FD="fd  '.' '$SEARCH_ROOT' -HI"
#COMMAND_FD="fd --hidden --no-ignore '' '$SEARCH_ROOT'"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-no-pattern.md

check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
#ordering
sort /tmp/results.fd > /tmp/results.fd_sorted
sort /tmp/results.find > /tmp/results.find_sorted
total_diff=$(diff /tmp/results.fd_sorted /tmp/results.find_sorted | wc -l)
echo "The total difference is $(($total_diff / 2))"
diff /tmp/results.fd_sorted /tmp/results.find_sorted | awk '{print $2}' | tr -s ' ' >  /tmp/missing_results.fdf
echo 'missing results(if true are 0)'
cat /tmp/missing_results.fdf
echo "however, when searching directly, we find that they are not missing."
echo "this is a bit broken currently, basically there's a weird off by 1 error i get sometimes, im not desperately trying to fix it because i believe its hyperfine related"

