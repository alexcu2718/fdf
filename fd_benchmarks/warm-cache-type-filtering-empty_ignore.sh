#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"


run_warm_benchmark "type-filtering-empty" "'.' '$SEARCH_ROOT' -H --type e" "'.' '$SEARCH_ROOT' -H --type e" "type_e"
