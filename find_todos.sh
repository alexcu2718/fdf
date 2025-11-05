#!/usr/bin/env bash

if command -v rg &> /dev/null; then
    rg '(todo|fixme)' ./src --ignore-case #god modern tools are nice.
else
    # Fallback to find+grep
    find . -maxdepth 2 -type f \( -name "*.rs" -o -name "*.sh" -o -name "README*" \) -not -name "find_todos.sh" -exec grep -ni "TODO\|FIXME" {} +
fi