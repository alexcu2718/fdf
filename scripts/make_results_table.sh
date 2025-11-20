#!/usr/bin/env bash

cd "$(dirname "$0" )"
cd ..
echo -e "Benchmark results for $(uname -a )\n\n"
printf "| %-50s | %-15s | %-15s | %-8s | %-16s |\n" "Test Case" "fdf Mean" "fd Mean" "Speedup" "Relative"
printf "| %-50s | %-15s | %-15s | %-8s | %-15s |\n" ":----------" ":--------:" ":-------:" ":-------:" ":--------:"

cat bench_results/*.md | grep -Ei '^\|' | grep -Ei -v '\|:' | grep -v 'Command.*Mean.*Min.*Max.*Relative' | \
awk -F'|' '
function trim(s) {
    gsub(/^[ \t]+|[ \t]+$/, "", s)
    return s
}
function format_cmd(cmd) {

    gsub(/^'"'"'|'"'"'$/, "", cmd)
    gsub(/^"|"$/, "", cmd)
    return cmd
}
{
    line = $0
    gsub(/^\| /, "", line)  # Remove leading "| "
    gsub(/ \|$/, "", line)  # Remove trailing " |"

    # Split into fields
    n = split(line, fields, " \\| ")

    if (n >= 5) {
        command = trim(fields[1])
        mean = trim(fields[2])
        relative = trim(fields[5])

        if (command ~ /^`fdf/) {
            # Extract command without program name
            cmd = command
            gsub(/^`fdf /, "", cmd)
            gsub(/`$/, "", cmd)
            cmd = format_cmd(cmd)
            fdf_mean = mean
            fdf_relative = relative
        }
        else if (command ~ /^`fd /) {
            fd_mean = mean
            fd_relative = relative

            if (fdf_mean != "") {
                # Extract numeric part before " ± "
                split(fdf_mean, fdf_parts, " ± ")
                split(fd_mean, fd_parts, " ± ")

                if (fdf_parts[1] + 0 > 0) {
                    speedup = sprintf("%.2fx", fd_parts[1] / fdf_parts[1])
                    printf "| %-50s | %-15s | %-15s | %-8s | %-15s |\n", "`" cmd "`", fdf_mean, fd_mean, "**" speedup "**", fd_relative
                } else {
                    printf "| %-50s | %-15s | %-15s | %-8s | %-15s |\n", "`" cmd "`", fdf_mean, fd_mean, "N/A", fd_relative
                }
                fdf_mean = ""
            }
        }
    }
}
' > results_table.md

cat results_table.md
