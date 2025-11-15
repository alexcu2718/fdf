#!/usr/bin/env bash


cd "$(dirname $0)"


WORD_SIZE=$(getconf LONG_BIT)

SETCD="$PWD"

LIBC_LOCATION="../libc"


if [ ! -d $LIBC_LOCATION ]; then
    git clone --depth 1 https://github.com/rust-lang/libc.git $LIBC_LOCATION
fi

[ -d $LIBC_LOCATION ] || { echo "$LIBC_LOCATION not found investigate this!"; exit 1; }

cd $LIBC_LOCATION && git pull  > /dev/null 2>&1 

cd $SETCD


rg 'pub struct dirent' -n $LIBC_LOCATION | cut -d: -f1 | sort -u | while read -r file; do
  echo "### $file" && awk '/pub struct dirent/ {found=1} found {print} /\}/ && found {found=0}' "$file" && echo
  ino_file=$(grep -l "type ino" "$file" 2>/dev/null || find $(dirname "$file") -name "*.rs" -exec grep -l "type ino" {} \; | head -1)
  [ -n "$ino_file" ] && echo "### $ino_file" && awk '/type ino/ {found=1} found {print} /;/ && found {found=0}' "$ino_file" && echo
done  > /dev/null 2>&1 > dirent_structs.txt

cat dirent_structs.txt
