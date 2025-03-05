#!/bin/bash

source "prelude.sh"

echo I HAVE MODIFIED THIS TO REPRESENT MY FILE EXTENSION SEARCH

EXT="jpg"

COMMAND_FIND="fdf -HI --extension '$EXT' '' '$SEARCH_ROOT'"
COMMAND_FD="fd -HI --extension '$EXT' '' '$SEARCH_ROOT'"


hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-file-extension.md

check_for_differences "false" "$COMMAND_FIND" "$COMMAND_FD"
