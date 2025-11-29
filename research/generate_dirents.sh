#!/usr/bin/env bash

# shellcheck disable=SC2001

cd "$(dirname "$0")" || exit

output_file="dirent_structs.txt"
#WORD_SIZE=$(getconf LONG_BIT) #unused, i might use it later

SETCD="$PWD"

LIBC_LOCATION="../libc"


if [ ! -d $LIBC_LOCATION ]; then
    git clone --depth 1 https://github.com/rust-lang/libc.git $LIBC_LOCATION
fi

[ -d $LIBC_LOCATION ] || { echo "$LIBC_LOCATION not found investigate this!"; exit 1; }

cd $LIBC_LOCATION && git pull  > /dev/null 2>&1

cd "$SETCD" || exit



rg -l --pcre2 -U '(?s)(pub struct dirent(64)?\s*\{.*?\}|pub type ino.*;)' $LIBC_LOCATION | while read -r file; do
    echo "## $file"

    ino_defs=$(rg -o 'pub type ino.*;' "$file" --no-heading 2>/dev/null)
    if [ -n "$ino_defs" ]; then
        echo "## ino types:"
        echo "$ino_defs" | sed 's/^/  /'
    fi

    dirent_defs=$(rg -o --pcre2 -U '(?s)pub struct dirent(64)?\s*\{.*?\}' "$file" --no-heading 2>/dev/null)
    if [ -n "$dirent_defs" ]; then
        echo "## dirent structs:"
        echo "$dirent_defs" | sed 's/^/  /'
    fi

    echo
done > $output_file

cat $output_file
