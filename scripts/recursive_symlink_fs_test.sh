#!/usr/bin/env bash

echo -e "IMPORTANT: PLEASE DELETE THE FILE THE FILE \n  OTHERWISE YOU'LL HAVE SOME FUNKY effects trying to do anything in this directory!\n"
cd "$(dirname "$0" )"
cd ..

TEST_DIR="looptest"
LOOP_DIR="test_loops"

ORIGINAL_DIR="$(pwd)"


mkdir -p "$LOOP_DIR"
cd "$LOOP_DIR" || exit 1
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR" || exit 1

# Create directory structure with recursive symlinks (these are all fake files )
mkdir -p \
    bin \
    usr/bin \
    var/tmp-ABC123/bin \
    var/tmp-ABC123/usr/bin \
    ".wine/dosdevices/z:/usr/lib/node_modules/eslint/node_modules"

# Create recursive symlinks
ln -sf ../bin                 bin/X11
ln -sf ../usr/bin             usr/bin/X11
ln -sf ../../bin              var/tmp-ABC123/bin/X11
ln -sf ../../usr/bin          var/tmp-ABC123/usr/bin/X11

# Create Wine-style recursive symlinks
ln -sf ../eslint              ".wine/dosdevices/z:/usr/lib/node_modules/eslint/node_modules/eslint"
ln -sf "$(cd .. && pwd)"      ".wine/dosdevices/z:/home"

# Additional test symlinks
ln -sf .. loopdir
ln -sf /  root_link

echo "This is a script to find  expose an infinite loop issue found in fd"
echo "fd may not terminate due to symlinks similar to those in ~/.steam or ~/.wine."
echo "This is an issue on my own PC, but you can emulate it with this"
echo "On most systems this will be fine, but on some it will show loop issues."

cd "$ORIGINAL_DIR" || exit 1

echo -e "\n Testing fd with recursive symlinks (timeout: 30 seconds)..."
if timeout 30 fd '.' -HIL "$LOOP_DIR/$TEST_DIR" --type l >/dev/null 2>&1; then
    echo "fd completed successfully within 10 seconds"
else
    status=$?
    if [ $status -eq 124 ]; then
        echo -e "fd -HIL timed out after 30 seconds (caught in infinite loop)\n"
        echo -e "This demonstrates the bug with recursive symlinks!\n"
    else
        echo "fd failed with exit code: $status"
    fi
fi


if [ -d "$LOOP_DIR" ]; then
    read -p "Remove $LOOP_DIR directory? (y/N): " remove_answer
    case "$remove_answer" in
        [yY])
            echo "Cleaning up test directory..."
            rm -rf "$LOOP_DIR"
            echo "Removed $LOOP_DIR directory"
            ;;
        *)
            echo "Keeping $LOOP_DIR directory for inspection."
            echo "You can examine it at: $(pwd)/$LOOP_DIR"
            ;;
    esac
else
    echo "Test directory $LOOP_DIR was already removed or never created."
fi

echo "Script completed."
