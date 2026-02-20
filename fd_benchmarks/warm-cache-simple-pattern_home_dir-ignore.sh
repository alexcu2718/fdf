#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"

pattern="'.*[0-9].*(md|\.c)$'"
run_warm_benchmark "simple-pattern_home_dir" "-H $pattern '$HOME'" "-H $pattern '$HOME'" "pattern_home_dir"
