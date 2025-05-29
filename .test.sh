

# Warmup runs to ensure caching doesn't affect results
WARMUP=5

# The actual benchmark runs
RUNS=10

# Command to test (counts files in current directory and subdirectories)
COMMAND=". / -HI --type e | wc -l"

echo "Benchmarking fdf_control..."
hyperfine \
  --warmup $WARMUP \
  --runs $RUNS \
  --export-markdown fdf_benchmark.md \
  --command-name "fdf_control" \
  "~/fdf_control/target/release/fdf $COMMAND"
# Command to test (counts files in current directory and subdirectories)
echo "Benchmarking fdf_og..."
hyperfine \
  --warmup $WARMUP \
  --runs $RUNS \
  --export-markdown fdf_benchmark.md \
  --command-name "fdf_og" \
  "~/fdf_og/target/release/fdf $COMMAND" \
  --style full


