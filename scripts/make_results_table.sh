#!/usr/bin/env bash


cd "$(dirname "$0" )" || exit
cd ..
echo -e "Benchmark results for $(uname -a )\n\n"
printf "| %-50s | %-15s | %-15s | %-9s | %-15s |\n" "Test Case" "fdf Mean" "fd Mean" "Speedup" "Relative"
printf "| %-50s | %-15s | %-15s | %-8s | %-15s |\n" ":----------" ":--------:" ":-------:" ":-------:" ":--------:"

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

    if (command ~ /^`fdf /) {
        key = command
        gsub(/^`fdf /, "", key)
        gsub(/`$/, "", key)
        key = format_cmd(key)
        fdf_mean[key] = mean
        if (!(key in order)) {
            order[key] = ++seq
            keys[seq] = key
        }
    } else if (command ~ /^`fd /) {
        key = command
        gsub(/^`fd /, "", key)
        gsub(/`$/, "", key)
        key = format_cmd(key)
        fd_mean[key] = mean
        fd_relative[key] = relative
        if (!(key in order)) {
            order[key] = ++seq
            keys[seq] = key
        }
    }
}
END {
    for (i = 1; i <= seq; i++) {
        key = keys[i]
        if ((key in fdf_mean) && (key in fd_mean)) {
            fdf_val = mean_value(fdf_mean[key])
            fd_val = mean_value(fd_mean[key])
            if (fdf_val > 0) {
                speedup_value = fd_val / fdf_val
                speedup = sprintf("%.2fx", speedup_value)
                speedup_sum += speedup_value
                speedup_count++
                printf "| %-50s | %-15s | %-15s | %-8s | %-15s |\n", "`" key "`", fdf_mean[key], fd_mean[key], "**" speedup "**", fd_relative[key]
            } else {
                printf "| %-50s | %-15s | %-15s | %-8s | %-15s |\n", "`" key "`", fdf_mean[key], fd_mean[key], "N/A", fd_relative[key]
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
