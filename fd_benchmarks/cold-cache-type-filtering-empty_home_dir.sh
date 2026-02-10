#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

run_cold_benchmark "type-filtering-empty_home_dir" "'.' '$HOME' -HI --type e" "'.' '$HOME' -HI --type e" "type_e_home_dir"
