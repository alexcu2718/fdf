#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

SIZE="+1mb"
run_cold_benchmark "size-test" "-HI --size '$SIZE' '' '$SEARCH_ROOT'" "-HI --size '$SIZE' '' '$SEARCH_ROOT'" "size"
