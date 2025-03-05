#!/bin/bash

source "prelude.sh"
echo I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.


COMMAND_FIND="fdf --hidden '' '$SEARCH_ROOT'"
COMMAND_FD="fd --hidden '' '$SEARCH_ROOT'"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-no-pattern.md

check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
