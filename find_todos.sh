#!/usr/bin/env bash
find . -maxdepth 2 -type f \( -name "*.rs" -o -name "*.sh" -o -name "README*" \) -not -name "find_todos.sh" -exec grep -i "TODO\|FIXME" {} +


