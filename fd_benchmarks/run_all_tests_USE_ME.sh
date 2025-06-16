#!/bin/bash

# Execute all warm*.sh scripts in the current directory
for script in ./warm*.sh; do
    ./"$script"
done
