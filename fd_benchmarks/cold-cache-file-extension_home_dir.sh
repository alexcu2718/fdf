#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

EXT="c"
echo "running extension test"
run_cold_benchmark "file-extension_home_dir" "-HI --extension '$EXT' '' '$HOME'" "-HI --extension '$EXT' '' '$HOME'" "extension_home_dir"
