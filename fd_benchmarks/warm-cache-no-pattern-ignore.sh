#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
run_warm_benchmark "no-pattern" "'.' '$SEARCH_ROOT' -H" "'.' '$SEARCH_ROOT' -H" "fdf"
