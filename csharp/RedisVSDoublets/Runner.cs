namespace Comparisons.RedisVSDoublets;

/// <summary>
/// Runs the benchmarks whose <c>Group/Implementation</c> name contains a filter and prints one line per benchmark in the
/// format of <c>cargo bench -- --output-format bencher</c>.
/// </summary>
public sealed class Runner(string filter)
{
    private static readonly Mode[] Modes = [Mode.NonTransaction, Mode.Transaction, Mode.Pipeline];

    private readonly int _background = Settings.BackgroundLinks;
    private readonly int _links = Settings.BenchmarkLinks;

    /// <summary>Runs the benchmarks of <paramref name="backend"/>: <c>redis</c>, <c>doublets</c> or both when empty.</summary>
    public void Run(string backend)
    {
        if (backend is not ("" or "redis" or "doublets"))
        {
            throw new ArgumentException($"BENCHMARK_BACKEND must be `redis` or `doublets`, got `{backend}`");
        }
        if (backend is "" or "redis")
        {
            RunRedis();
        }
        if (backend is "" or "doublets")
        {
            RunDoublets();
        }
    }

    private void RunRedis()
    {
        foreach (var (group, mode) in Benchmarks.Groups.SelectMany(group => Modes.Select(mode => (group, mode))))
        {
            Run<IBenchedLinks>(group, $"Redis_{mode}", Sampling.Redis, () => RedisLinks.Open(mode), Benchmarks.Single(group, _background, _links), Benchmarks.CheckBackground);
        }
        foreach (var (group, mode) in UsageBenchmarks.Groups.SelectMany(group => Modes.Select(mode => (group, mode))))
        {
            Run<IBenchedUsage>(group, $"Redis_{mode}", Sampling.Redis, () => RedisUsage.Open(mode), UsageBenchmarks.Work(group, _background, _links), UsageBenchmarks.CheckBackground);
        }
    }

    private void RunDoublets()
    {
        foreach (var (group, variant) in Benchmarks.Groups.SelectMany(group => DoubletsLinks.Variants.Select(variant => (group, variant))))
        {
            Run<IBenchedLinks>(group, variant, Sampling.Doublets, () => DoubletsLinks.Open(variant), Benchmarks.Single(group, _background, _links), Benchmarks.CheckBackground);
        }
        foreach (var group in UsageBenchmarks.Groups)
        {
            Run<IBenchedUsage>(group, DoubletsUsage.Variant, Sampling.Doublets, () => new DoubletsUsage(), UsageBenchmarks.Work(group, _background, _links), UsageBenchmarks.CheckBackground);
        }
    }

    private void Run<TStore>(string group, string id, Sampling sampling, Func<TStore> open, Work<TStore> work, Action<TStore, int> checkBackground)
        where TStore : IBenched
    {
        if (!$"{group}/{id}".Contains(filter, StringComparison.Ordinal))
        {
            return;
        }
        Console.Error.WriteLine($"Benchmarking {group}/{id}");
        using var store = open();
        var estimate = Harness.Measure(sampling, store, work, checkBackground);
        Console.WriteLine(estimate.Bencher(group, id));
    }
}
