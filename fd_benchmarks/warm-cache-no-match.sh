#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"
PATTERN="THISSHOULDNEVERMATCH"
run_warm_benchmark "no-match" "$PATTERN '$SEARCH_ROOT' -HI" "$PATTERN '$SEARCH_ROOT' -HI" "no-match" 1
