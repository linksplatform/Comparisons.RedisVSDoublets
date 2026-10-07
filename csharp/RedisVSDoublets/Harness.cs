using System.Diagnostics;
using System.Globalization;

namespace Comparisons.RedisVSDoublets;

public readonly record struct Estimate(double Median, double StandardDeviation)
{
    public static Estimate Of(IReadOnlyCollection<double> samples)
    {
        var sorted = samples.Order().ToArray();
        var middle = sorted.Length / 2;
        var median = sorted.Length % 2 == 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
        var mean = sorted.Average();
        var variance = sorted.Sum(time => (time - mean) * (time - mean)) / (sorted.Length - 1);
        return new Estimate(median, Math.Sqrt(variance));
    }

    public string Bencher(string operation, string variant) => string.Create(CultureInfo.InvariantCulture,
        $"test {operation}/{variant} ... bench: {Math.Max(1, Math.Round(Median)):F0} ns/iter (+/- {Math.Round(StandardDeviation):F0})");
}

public static class Harness
{
    public static Estimate Measure(IStore store, int operation, Settings settings)
    {
        var background = (ulong)settings.Background;
        var links = (ulong)settings.Links;
        for (var warmup = 0; warmup < 3; warmup++)
        {
            Workload.Exercise(store, operation, background, links);
            Workload.Undo(store, operation, background, links);
        }
        var times = new double[settings.Samples];
        for (var sample = 0; sample < settings.Samples; sample++)
        {
            var started = Stopwatch.GetTimestamp();
            Workload.Exercise(store, operation, background, links);
            times[sample] = Stopwatch.GetElapsedTime(started).TotalNanoseconds;
            Workload.Undo(store, operation, background, links);
        }
        return Estimate.Of(times);
    }
}
