#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"

run_warm_benchmark "type-filtering-directory_home_dir" "'.' '$HOME' -HI --type d" "'.' '$HOME' -HI --type d" "type_d_home_dir"
