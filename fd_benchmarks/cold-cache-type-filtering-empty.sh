#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

run_cold_benchmark "type-filtering-empty" "'.' '$SEARCH_ROOT' -HI --type e" "'.' '$SEARCH_ROOT' -HI --type e" "type_e"
