❯ ./warm-cache-relative-dir-test.sh
I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.
Benchmark 1: fdf '.' '..' -HI
  Time (mean ± σ):       2.6 ms ±   0.4 ms    [User: 1.3 ms, System: 8.7 ms]
  Range (min … max):     2.2 ms …   5.2 ms    924 runs

  Warning: Command took less than 5 ms to complete. Note that the results might be inaccurate because hyperfine can not calibrate the shell startup time much more precise than this limit. You can try to use the `-N`/`--shell=none` option to disable the shell completely.
  Warning: Statistical outliers were detected. Consider re-running this benchmark on a quiet system without any interferences from other programs. It might help to use the '--warmup' or '--prepare' options.

Benchmark 2: fd '.' '..' -HI
  Time (mean ± σ):       7.3 ms ±   0.7 ms    [User: 3.6 ms, System: 7.8 ms]
  Range (min … max):     6.2 ms …  10.9 ms    331 runs

Summary
  fdf '.' '..' -HI ran
    2.79 ± 0.55 times faster than fd '.' '..' -HI
WARNING: There were differences between the search results of fd and find!
Run 'diff /tmp/results.fd /tmp/results.find'.
the count of files in the results.fd are 985
the count of files in the results.find are 984
the total difference are 2
❯ diff /tmp/results.fd /tmp/results.find
70d69
< ../README.md
❯ ./warm-cache-depth-test.sh
I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.
Benchmark 1: fdf '.' '/home/alexc' -HI -d 2
  Time (mean ± σ):       2.6 ms ±   0.4 ms    [User: 1.2 ms, System: 8.4 ms]
  Range (min … max):     2.3 ms …   5.3 ms    814 runs

  Warning: Command took less than 5 ms to complete. Note that the results might be inaccurate because hyperfine can not calibrate the shell startup time much more precise than this limit. You can try to use the `-N`/`--shell=none` option to disable the shell completely.
  Warning: Statistical outliers were detected. Consider re-running this benchmark on a quiet system without any interferences from other programs. It might help to use the '--warmup' or '--prepare' options.

Benchmark 2: fd '.' '/home/alexc' -HI -d 2
  Time (mean ± σ):       6.7 ms ±   0.7 ms    [User: 3.8 ms, System: 9.3 ms]
  Range (min … max):     5.5 ms …   9.1 ms    367 runs

Summary
  fdf '.' '/home/alexc' -HI -d 2 ran
    2.56 ± 0.51 times faster than fd '.' '/home/alexc' -HI -d 2
WARNING: There were differences between the search results of fd and find!
Run 'diff /tmp/results.fd /tmp/results.find'.
the count of files in the results.fd are 1099
the count of files in the results.find are 1098
the total difference are 2
❯ diff /tmp/results.fd /tmp/results.find
1070d1069
< /home/alexc/.xonshrc
❯ ./warm-cache-relative-dir-test.sh
I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.
Benchmark 1: fdf '.' '..' -HI
  Time (mean ± σ):       2.6 ms ±   0.4 ms    [User: 1.3 ms, System: 8.8 ms]
  Range (min … max):     2.3 ms …   4.3 ms    924 runs

  Warning: Command took less than 5 ms to complete. Note that the results might be inaccurate because hyperfine can not calibrate the shell startup time much more precise than this limit. You can try to use the `-N`/`--shell=none` option to disable the shell completely.
  Warning: Statistical outliers were detected. Consider re-running this benchmark on a quiet system without any interferences from other programs. It might help to use the '--warmup' or '--prepare' options.

Benchmark 2: fd '.' '..' -HI
  Time (mean ± σ):       7.3 ms ±   0.6 ms    [User: 3.5 ms, System: 7.9 ms]
  Range (min … max):     6.3 ms …   9.2 ms    329 runs

Summary
  fdf '.' '..' -HI ran
    2.78 ± 0.51 times faster than fd '.' '..' -HI
WARNING: There were differences between the search results of fd and find!
Run 'diff /tmp/results.fd /tmp/results.find'.
the count of files in the results.fd are 986
the count of files in the results.find are 985
the total difference are 2
❯ diff /tmp/results.fd /tmp/results.find
71d70
< ../README.md
❯ code .
❯ fdf . .. -HI -d 2 | grep READ
../README.md
../fd_benchmarks/README.md
❯
❯ fdf . / -HI | grep ^/home/alexc/.xonsh
/home/alexc/.xonshrc
~/scanit_versions/fdf/fd_benchmarks main !8 ?4 ❯                                                                                                                                      17:07:36
