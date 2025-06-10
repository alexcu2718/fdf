#!/bin/bash

source "prelude.sh"

source "new_prelude.sh"
pattern="'.*[0-9].*(md|\.c)$'"

COMMAND_FIND="fdf -HI $pattern '$SEARCH_ROOT'"
COMMAND_FD="fd -HI $pattern '$SEARCH_ROOT'"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-simple-pattern.md

check_for_differences "false" "$COMMAND_FIND" "$COMMAND_FD"
