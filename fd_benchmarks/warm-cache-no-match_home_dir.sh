#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"
PATTERN="THISSHOULDNEVERMATCH"
run_warm_benchmark "no-match-home" "$PATTERN '$HOME' -HI" "$PATTERN '$HOME' -HI" "no-match-home" 1
