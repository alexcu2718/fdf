#!/bin/bash

source "prelude.sh"


source "new_prelude.sh"

COMMAND_FIND_SL="fdf .  '$SEARCH_ROOT' -HI --type l"
COMMAND_FD_SL="fd -HI '' '$SEARCH_ROOT' --type l"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND_SL" \
    "$COMMAND_FD_SL" \
    --export-markdown results-warm-cache-type-filtering.md

check_for_differences "false" "$COMMAND_FIND_SL" "$COMMAND_FD_SL"



COMMAND_FIND_EMPTY="fdf .  '$SEARCH_ROOT' -HI --type e"
COMMAND_FD_EMPTY="fd -HI '' '$SEARCH_ROOT' --type e"

hyperfine --warmup "$WARMUP_COUNT" \
     "$COMMAND_FIND_EMPTY" \
     "$COMMAND_FD_EMPTY" \
     --export-markdown results-warm-cache-type-filtering.md

check_for_differences "false" "$COMMAND_FIND_EMPTY" "$COMMAND_FD_EMPTY"

