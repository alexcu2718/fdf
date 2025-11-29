#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"
run_warm_benchmark "no-pattern_home_dir" "'.' '$HOME' -HI" "'.' '$HOME' -HI" "fdf_home_dir"
