#!/usr/bin/env bash


echo "this is a script to find recursive symlinks on your computer, fd doesn't terminate on my own due to ~/.steam and ~/.wine"
echo "essentially this will just show what symlinks recursive"
find -L "$HOME" -xdev -readable -type l \
  -printf 'Inode: %i Symlink: %p -> %l (symlink length: %s bytes)\n' \
  2> >(while IFS= read -r line; do
         if printf "%s\n" "$line" | grep -q "Permission denied"; then
             continue
         fi
         path=$(printf "%s\n" "$line" | sed -n "s/find: ‘\(.*\)’: .*/\1/p")
         if [ -n "$path" ]; then
             len=${#path}
          
             if inode=$(stat -c %i "$path" 2>/dev/null); then
                 printf "%s [path length: %d, inode: %s]\n" "$line" "$len" "$inode" >&2
             else
                 printf "%s [path length: %d, inode: unknown]\n" "$line" "$len" >&2
             fi
         else
             printf "%s\n" "$line" >&2
         fi
       done)
