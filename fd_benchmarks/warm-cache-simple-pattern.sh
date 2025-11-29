#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"

pattern="'.*[0-9].*(md|\.c)$'"
run_warm_benchmark "simple-pattern" "-HI $pattern '$SEARCH_ROOT'" "-HI $pattern '$SEARCH_ROOT'" "pattern"
