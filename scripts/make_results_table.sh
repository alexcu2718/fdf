#!/usr/bin/env bash

# This script parses the Markdown tables produced by `hyperfine --export-markdown`.
# The column layout of that export changed in hyperfine 2.0.0 (Command | Mean | Min | Max | Relative
# became Command | Mean [metric] | Change | Factor), so a minimum version is required here.
MIN_HYPERFINE_VERSION="2.0.0"

version_ge() {
    # true (0) if $1 >= $2
    [ "$1" = "$2" ] && return 0
    [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$2" ]
}

check_hyperfine_version() {
    if ! command -v hyperfine >/dev/null 2>&1; then
        echo "Error: hyperfine is not installed. This script requires hyperfine >= ${MIN_HYPERFINE_VERSION}." >&2
        echo "       Install it from https://github.com/sharkdp/hyperfine#installation" >&2
        exit 1
    fi

    local installed_version
    installed_version=$(hyperfine --version | awk '{print $2}')

    if ! version_ge "$installed_version" "$MIN_HYPERFINE_VERSION"; then
        echo "Error: found hyperfine ${installed_version}, but this script requires hyperfine >= ${MIN_HYPERFINE_VERSION}." >&2
        echo "       was introduced in hyperfine 2.0.0 and is incompatible with older 1.x exports." >&2
        echo "       Please upgrade hyperfine: https://github.com/sharkdp/hyperfine#installation" >&2
        exit 1
    fi
}

check_hyperfine_version

cd "$(dirname "$0" )" || exit
cd ..
echo -e "Benchmark results for $(uname -a )\n\n"
printf "| %-70s | %-15s | %-15s | %-9s | %-15s |\n" "Test Case" "fdf Mean" "fd Mean" "Speedup" "Relative"
printf "| %-70s | %-15s | %-15s | %-9s | %-15s |\n" ":----------" ":--------:" ":-------:" ":-------:" ":--------:"

mapfile -d '' files < <(find bench_results -type f -name '*.md' -print0 | sort -z)

awk '
function trim(s) {
    gsub(/^[ \t]+|[ \t]+$/, "", s)
    return s
}
function format_cmd(cmd) {

    gsub(/^'"'"'|'"'"'$/, "", cmd)
    gsub(/^"|"$/, "", cmd)
    return cmd
}
function case_label_from_filename(path,    parts, filename, label, count) {
    count = split(path, parts, "/")
    filename = parts[count]
    gsub(/\.md$/, "", filename)
    gsub(/^results-/, "", filename)
    if (filename ~ /cold-cache/) {
        label = "cold-cache"
    } else if (filename ~ /warm-cache/) {
        label = "warm-cache"
    } else {
        label = filename
    }
    return label
}
BEGIN {
    speedup_sum = 0
    speedup_count = 0
    seq = 0
}

function mean_value(s,    parts) {
    split(s, parts, " ± ")
    return parts[1] + 0
}
{
    if ($0 !~ /^\|/) {
        next
    }
    if ($0 ~ /\|:/) {
        next
    }
    if ($0 ~ /Command.*Mean.*Change.*Factor/) {
        next
    }

    # hyperfine >= 2.0.0 markdown rows look like:
    # | `command` | mean ± stddev | change | factor |
    # Splitting on a bare "|" (rather than " | ") keeps empty cells (e.g. the
    # Change/Factor columns on the reference row) from swallowing neighbouring fields.
    # Benchmarked commands can themselves contain a literal escaped pipe (e.g. a regex
    # pattern with "\|"), so mask those before splitting and restore them afterwards.
    line = $0
    gsub(/\\\|/, "\001", line)
    n = split(line, fields, "|")
    for (i = 1; i <= n; i++) {
        gsub(/\001/, "\\|", fields[i])
    }
    if (n < 5) {
        next
    }

    command = trim(fields[2])
    mean = trim(fields[3])
    change = trim(fields[4])
    factor = trim(fields[5])
    relative = change
    if (factor != "") {
        relative = relative " " factor
    }
    is_cold_cache_file = (FILENAME ~ /cold-cache/)
    case_label = case_label_from_filename(FILENAME)

    if (command ~ /^`fdf /) {
        key = command
        gsub(/^`fdf /, "", key)
        gsub(/`$/, "", key)
        key = format_cmd(key)
        composite_key = case_label SUBSEP key
        fdf_mean[composite_key] = mean
        if (is_cold_cache_file) {
            cold_cache_key[composite_key] = 1
        }
        if (!(composite_key in order)) {
            order[composite_key] = ++seq
            keys[seq] = composite_key
            labels[composite_key] = case_label
            commands[composite_key] = key
        }
    } else if (command ~ /^`fd /) {
        key = command
        gsub(/^`fd /, "", key)
        gsub(/`$/, "", key)
        key = format_cmd(key)
        composite_key = case_label SUBSEP key
        fd_mean[composite_key] = mean
        fd_relative[composite_key] = relative
        if (is_cold_cache_file) {
            cold_cache_key[composite_key] = 1
        }
        if (!(composite_key in order)) {
            order[composite_key] = ++seq
            keys[seq] = composite_key
            labels[composite_key] = case_label
            commands[composite_key] = key
        }
    }
}
END {
    for (i = 1; i <= seq; i++) {
        composite_key = keys[i]
        if ((composite_key in fdf_mean) && (composite_key in fd_mean)) {
            fdf_val = mean_value(fdf_mean[composite_key])
            fd_val = mean_value(fd_mean[composite_key])
            is_cold_cache = (composite_key in cold_cache_key)
            display_key = labels[composite_key] " `" commands[composite_key] "`"
            if (fdf_val > 0) {
                speedup_value = fd_val / fdf_val
                if (speedup_value >= 1) {
                    speedup = sprintf("%.2fx", speedup_value)
                } else {
                    speedup = sprintf("%.2fx slower", 1 / speedup_value)
                }
                speedup_sum += speedup_value
                speedup_count++
                printf "| %-70s | %-15s | %-15s | %-9s | %-15s |\n", display_key, fdf_mean[composite_key], fd_mean[composite_key], speedup, fd_relative[composite_key]
            } else {
                printf "| %-70s | %-15s | %-15s | %-9s | %-15s |\n", display_key, fdf_mean[composite_key], fd_mean[composite_key], "N/A", fd_relative[composite_key]
            }
        }
    }
    if (speedup_count > 0) {
        avg_speedup = sprintf("%.2fx", speedup_sum / speedup_count)
        printf "\n**Average Speedup: %s**\n", avg_speedup
    }
}
' "${files[@]}" > results_table.md

cat results_table.md
