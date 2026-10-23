// Usage: redis-vs-doublets [FILTER]
//
// Runs the benchmarks of both databases, or only of the one named by the `BENCHMARK_BACKEND`
// environment variable (`redis` or `doublets`), and prints one line per benchmark in the format
// of `cargo bench -- --output-format bencher`. Only benchmarks whose `Group/Implementation` name
// contains FILTER run. The sizes come from `BENCHMARK_BACKGROUND_LINKS` and `BENCHMARK_LINKS`,
// the Redis server from `REDIS_URL` (default `redis://localhost:6379`), whose database is emptied.

using Comparisons.RedisVSDoublets;

new Runner(args.Length > 0 ? args[0] : "").Run(Settings.Backend);

// Keeps the visited links and values alive, so reading them cannot be optimized away.
Console.Error.WriteLine($"Visited values sum to {Benchmarks.Visited}");
