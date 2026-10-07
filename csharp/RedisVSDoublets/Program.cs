using System.Diagnostics;
using System.Globalization;

namespace Comparisons.RedisVSDoublets;

public static class Program
{
    public static void Main()
    {
        static int Setting(string name, int fallback) => Environment.GetEnvironmentVariable(name) is { } value
            ? int.Parse(value, CultureInfo.InvariantCulture) : fallback;
        var backend = Environment.GetEnvironmentVariable("BENCHMARK_BACKEND") ?? "doublets";
        var background = Setting("BENCHMARK_BACKGROUND_LINKS", 1000);
        var links = Setting("BENCHMARK_LINKS", 100);
        var samples = Setting("BENCHMARK_SAMPLES", 10);
        if (links <= 0 || links > background || samples < 3 || background > 1040383 - links)
            throw new ArgumentException("require 0 < links <= background, background + links <= 1040383, samples >= 3");
        using IStore store = backend switch
        {
            "redis" => new RedisStore(),
            "doublets" => new DoubletsStore(),
            _ => throw new ArgumentException("BENCHMARK_BACKEND must be redis or doublets"),
        };
        if (store is RedisStore redis) Console.WriteLine($"# redis: {redis.Version}");
        for (var i = 0; i < background; i++) store.Create();
        var variant = backend == "redis" ? "Redis" : "Doublets";
        for (var operation = 0; operation < Workload.Operations.Length; operation++)
        {
            for (var warmup = 0; warmup < 3; warmup++)
            {
                Workload.Exercise(store, operation, (ulong)background, (ulong)links);
                Workload.Undo(store, operation, (ulong)background, (ulong)links);
            }
            var times = new double[samples];
            for (var sample = 0; sample < samples; sample++)
            {
                var started = Stopwatch.GetTimestamp();
                Workload.Exercise(store, operation, (ulong)background, (ulong)links);
                times[sample] = Stopwatch.GetElapsedTime(started).TotalNanoseconds;
                Workload.Undo(store, operation, (ulong)background, (ulong)links);
            }
            Array.Sort(times);
            var median = samples % 2 == 0 ? (times[samples / 2 - 1] + times[samples / 2]) / 2 : times[samples / 2];
            var mean = times.Average();
            var deviation = Math.Sqrt(times.Sum(time => (time - mean) * (time - mean)) / (samples - 1));
            Console.WriteLine(string.Create(CultureInfo.InvariantCulture,
                $"test {Workload.Operations[operation]}/{variant} ... bench: {Math.Max(1, Math.Round(median)):F0} ns/iter (+/- {Math.Round(deviation):F0})"));
        }
    }
}
