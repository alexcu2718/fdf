#!/usr/bin/env bash


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
    if ($0 ~ /Command.*Mean.*Min.*Max.*Relative/) {
        next
    }

    line = $0
    gsub(/^\|[ ]*/, "", line)
    gsub(/[ ]*\|[ ]*$/, "", line)
    n = split(line, fields, " \\| ")
    if (n < 5) {
        next
    }

    command = trim(fields[1])
    mean = trim(fields[2])
    relative = trim(fields[5])
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
