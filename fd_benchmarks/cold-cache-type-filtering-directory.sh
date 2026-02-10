#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

run_cold_benchmark "type-filtering-directory" "'.' '$SEARCH_ROOT' -HI --type d" "'.' '$SEARCH_ROOT' -HI --type d" "type_d"
