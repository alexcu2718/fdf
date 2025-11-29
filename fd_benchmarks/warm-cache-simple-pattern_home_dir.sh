#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"

pattern="'.*[0-9].*(md|\.c)$'"
run_warm_benchmark "simple-pattern_home_dir" "-HI $pattern '$HOME'" "-HI $pattern '$HOME'" "pattern_home_dir"
