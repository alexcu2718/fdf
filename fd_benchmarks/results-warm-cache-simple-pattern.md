| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `fdf -HI '.*[0-9].*(md\|\.c)$' '/tmp/llvm-project'` | 27.6 ± 1.4 | 24.6 | 30.8 | 1.00 |
| `fd -HI '.*[0-9].*(md\|\.c)$' '/tmp/llvm-project'` | 46.1 ± 1.3 | 43.1 | 49.2 | 1.67 ± 0.10 |
