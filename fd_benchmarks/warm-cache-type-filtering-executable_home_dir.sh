#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"

run_warm_benchmark "type-filtering-executable_home_dir" "'.' '$HOME' -HI --type x" "'.' '$HOME' -HI --type x" "type_x_home_dir"
