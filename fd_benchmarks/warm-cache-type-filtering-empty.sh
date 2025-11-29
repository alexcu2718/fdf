#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"


run_warm_benchmark "type-filtering-empty" "'.' '$SEARCH_ROOT' -HI --type e" "'.' '$SEARCH_ROOT' -HI --type e" "type_e"
