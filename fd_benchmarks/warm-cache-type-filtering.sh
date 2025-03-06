#!/bin/bash

source "prelude.sh"

COMMAND_FIND="fdf .  '$SEARCH_ROOT' -HI --type l"
COMMAND_FD="fd -HI '' '$SEARCH_ROOT' --type l"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-type-filtering.md

check_for_differences "false" "$COMMAND_FIND" "$COMMAND_FD"

