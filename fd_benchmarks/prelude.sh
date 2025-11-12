#!/usr/bin/env bash


cd "$(dirname "$0")" || exit
source "config.sh"



ask_for_sudo() {
    echo "This script will now ask for your password in order to gain root/sudo"
    echo "permissions. These are required to reset the harddisk caches in between"
    echo "benchmark runs."
    echo ""

    sudo echo "Okay, acquired superpowers :-)" || exit

    echo ""
}
