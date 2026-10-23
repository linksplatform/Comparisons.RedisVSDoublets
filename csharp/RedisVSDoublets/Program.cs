// Usage: redis-vs-doublets [FILTER]
//
// Runs the benchmarks of both databases, or only of the one named by the `BENCHMARK_BACKEND`
// environment variable (`redis` or `doublets`), and prints one line per benchmark in the format
// of `cargo bench -- --output-format bencher`. Only benchmarks whose `Group/Implementation` name
// contains FILTER run. The sizes come from `BENCHMARK_BACKGROUND_LINKS` and `BENCHMARK_LINKS`,
// the Redis server from `REDIS_URL` (default `redis://localhost:6379`), whose database is emptied.

using Comparisons.RedisVSDoublets;

var filter = args.Length > 0 ? args[0] : "";
var backend = Settings.Backend;
if (backend is not ("" or "redis" or "doublets"))
{
    throw new ArgumentException($"BENCHMARK_BACKEND must be `redis` or `doublets`, got `{backend}`");
}
var (background, links) = (Settings.BackgroundLinks, Settings.BenchmarkLinks);
Mode[] modes = [Mode.NonTransaction, Mode.Transaction, Mode.Pipeline];

void Run<TStore>(string group, string id, Sampling sampling, Func<TStore> open, Work<TStore> work)
    where TStore : IBenched
{
    if (!$"{group}/{id}".Contains(filter, StringComparison.Ordinal))
    {
        return;
    }
    Console.Error.WriteLine($"Benchmarking {group}/{id}");
    using var store = open();
    var estimate = Harness.Measure(sampling, store, work.Operation, work.Undo);
    Console.WriteLine(estimate.Bencher(group, id));
}

if (backend is "" or "redis")
{
    foreach (var group in Benchmarks.Groups)
    {
        foreach (var mode in modes)
        {
            Run<IBenchedLinks>(group, $"Redis_{mode}", Sampling.Redis, () => RedisLinks.Open(mode), Benchmarks.Single(group, background, links));
        }
    }
    foreach (var group in UsageBenchmarks.Groups)
    {
        foreach (var mode in modes)
        {
            Run<IBenchedUsage>(group, $"Redis_{mode}", Sampling.Redis, () => RedisUsage.Open(mode), UsageBenchmarks.Work(group, background, links));
        }
    }
}

if (backend is "" or "doublets")
{
    foreach (var group in Benchmarks.Groups)
    {
        foreach (var variant in DoubletsLinks.Variants)
        {
            Run<IBenchedLinks>(group, variant, Sampling.Doublets, () => DoubletsLinks.Open(variant), Benchmarks.Single(group, background, links));
        }
    }
    foreach (var group in UsageBenchmarks.Groups)
    {
        Run<IBenchedUsage>(group, DoubletsUsage.Variant, Sampling.Doublets, () => new DoubletsUsage(), UsageBenchmarks.Work(group, background, links));
    }
}

// Keeps the visited links and values alive, so reading them cannot be optimized away.
Console.Error.WriteLine($"Visited values sum to {Benchmarks.Visited}");
