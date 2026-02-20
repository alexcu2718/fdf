#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
EXT="c"
echo "running extension test"
run_warm_benchmark "file-extension_home_dir" "-H --extension '$EXT' '' '$HOME'" "-H --extension '$EXT' '' '$HOME'" "extension_home_dir"
