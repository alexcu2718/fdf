#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

pattern="'.*[0-9].*(md|\.c)$'"
run_cold_benchmark "simple-pattern_home_dir" "-HI $pattern '$HOME'" "-HI $pattern '$HOME'" "pattern_home_dir"
