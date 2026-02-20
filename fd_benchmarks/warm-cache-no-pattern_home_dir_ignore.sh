#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
run_warm_benchmark "no-pattern_home_dir" "'.' '$HOME' -H" "'.' '$HOME' -H" "fdf_home_dir"
