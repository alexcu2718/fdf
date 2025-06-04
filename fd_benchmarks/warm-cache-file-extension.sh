#!/bin/bash

source "prelude.sh"

echo I HAVE MODIFIED THIS TO REPRESENT MY FILE EXTENSION SEARCH

EXT="jpg"

COMMAND_FIND="fdf -HI --extension '$EXT' '' '$SEARCH_ROOT'"
COMMAND_FD="fd -HI --extension '$EXT' '' '$SEARCH_ROOT'"


hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-file-extension.md

### my extension finder works different to fd, basically i found out#
### my detector doesnt find  eg /home/alexc/.cache/paru/clone/electron20/src/src/third_party/ffmpeg/tests/ref/lavf/jpg
##this is because it doesnt have a ., mine specifically looks for a dot, which how an extension
##SHOULD be defined, whatever


check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
echo  "the difference between the results is shown below, it should be 1 file, an edgecase i dont know how to fix AHHH"
echo $(( $(diff /tmp/results.fd /tmp/results.find | wc -l) / 2 ))
