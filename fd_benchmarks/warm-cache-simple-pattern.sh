#!/bin/bash

source "prelude.sh"

source "new_prelude.sh"
pattern="\.md$"
COMMAND_FIND="fdf -HI $pattern '$SEARCH_ROOT'"
COMMAND_FIND2="fdf  $pattern '$SEARCH_ROOT'"
COMMAND_FD="fd -HI $pattern '$SEARCH_ROOT'"
COMMAND_FD2="fd  $pattern  '$SEARCH_ROOT'"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FIND2" \
    "$COMMAND_FD" \
    "$COMMAND_FD2" \
    --export-markdown results-warm-cache-simple-pattern.md

check_for_differences "false" "$COMMAND_FIND" "$COMMAND_FD"
