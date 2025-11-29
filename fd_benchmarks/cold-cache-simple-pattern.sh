#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

pattern="'.*[0-9]\.jpg$'"
run_cold_benchmark "simple-pattern" "-HI $pattern '$SEARCH_ROOT'" "-HI $pattern '$SEARCH_ROOT'" "cold_pattern"
