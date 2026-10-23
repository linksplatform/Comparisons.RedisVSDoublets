using System.Diagnostics;
using System.Globalization;

namespace Comparisons.RedisVSDoublets;

/// <summary>
/// How a benchmark is sampled. The values are the ones of the Rust benchmarks (<c>rust/benches/benchmarks/mod.rs</c>),
/// and the iteration counts are chosen as Criterion 0.8 chooses them, so both languages measure the same way.
/// </summary>
/// <param name="SampleSize">Number of samples.</param>
/// <param name="Flat">Every sample has the same number of iterations (Criterion's flat sampling);
/// otherwise sample <c>i</c> has <c>i × d</c> iterations (linear sampling).</param>
/// <param name="WarmUp">How long the benchmark runs before it is measured.</param>
/// <param name="MeasurementTime">The measured time that all samples take together.</param>
public sealed record Sampling(int SampleSize, bool Flat, TimeSpan WarmUp, TimeSpan MeasurementTime)
{
    /// <summary>
    /// One Redis iteration waits for at least one network round trip, so it takes from tens of microseconds to milliseconds;
    /// 10 samples of the same number of iterations already give stable results.
    /// </summary>
    public static Sampling Redis { get; } = new(10, true, TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(5));

    /// <summary>One Doublets iteration takes microseconds, so 100 samples are used to measure it precisely.</summary>
    public static Sampling Doublets { get; } = new(100, false, TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(2));

    /// <summary>The number of iterations of every sample, for iterations that take <paramref name="nanoseconds"/> each.</summary>
    public long[] IterationCounts(double nanoseconds)
    {
        var measurement = MeasurementTime.TotalNanoseconds;
        if (Flat)
        {
            var iterations = Math.Max(1, (long)Math.Ceiling(measurement / (nanoseconds * SampleSize)));
            return Enumerable.Repeat(iterations, SampleSize).ToArray();
        }
        var totalRuns = (double)SampleSize * (SampleSize + 1) / 2;
        var step = Math.Max(1, (long)Math.Ceiling(measurement / nanoseconds / totalRuns));
        return Enumerable.Range(1, SampleSize).Select(sample => sample * step).ToArray();
    }
}

/// <summary>The median time of one iteration and the standard deviation of the samples, in nanoseconds.</summary>
public readonly record struct Estimate(double Median, double StandardDeviation)
{
    /// <summary>Estimates the time of one iteration from the average iteration time of every sample.</summary>
    public static Estimate Of(IReadOnlyCollection<double> samples)
    {
        var sorted = samples.Order().ToArray();
        var middle = sorted.Length / 2;
        var median = sorted.Length % 2 == 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
        var mean = sorted.Average();
        var variance = sorted.Length > 1 ? sorted.Sum(sample => (sample - mean) * (sample - mean)) / (sorted.Length - 1) : 0;
        return new Estimate(median, Math.Sqrt(variance));
    }

    /// <summary>
    /// The line that Criterion prints with <c>--output-format bencher</c>, for example
    /// <c>test Create/Redis_Transaction ... bench:     244,055 ns/iter (+/- 5,345)</c>.
    /// </summary>
    public string Bencher(string group, string id) =>
        string.Create(CultureInfo.InvariantCulture,
            $"test {group}/{id} ... bench: {Math.Round(Median),11:N0} ns/iter (+/- {Math.Round(StandardDeviation):N0})");
}

public static class Harness
{
    /// <summary>
    /// Measures <paramref name="iteration"/>, which runs one iteration and returns its measured time:
    /// first warms up for <see cref="Sampling.WarmUp"/>, doubling the number of iterations each round,
    /// then takes <see cref="Sampling.SampleSize"/> samples.
    /// </summary>
    public static Estimate Measure(Sampling sampling, Func<TimeSpan> iteration)
    {
        TimeSpan Run(long iterations)
        {
            var elapsed = TimeSpan.Zero;
            for (long i = 0; i < iterations; i++)
            {
                elapsed += iteration();
            }
            return elapsed;
        }

        var warmUp = TimeSpan.Zero;
        long warmUpIterations = 0;
        for (long iterations = 1; warmUp < sampling.WarmUp; iterations *= 2)
        {
            warmUp += Run(iterations);
            warmUpIterations += iterations;
        }
        var counts = sampling.IterationCounts(Math.Max(1, warmUp.TotalNanoseconds / warmUpIterations));
        return Estimate.Of(counts.Select(count => Run(count).TotalNanoseconds / count).ToArray());
    }

    /// <summary>
    /// Measures the operation of <paramref name="work"/> on <paramref name="store"/>, which contains
    /// <see cref="Settings.BackgroundLinks"/> background links (or keys, list items and set members). After every iteration
    /// the undo of <paramref name="work"/> puts the store back into the same state, so every iteration starts from the same
    /// data. Neither the background data nor the undo are measured.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The measured time starts before <see cref="IBenched.Begin"/> and ends after <see cref="IBenched.Commit"/>,
    /// so for Redis in transaction and pipeline modes it includes sending the queued commands and receiving all replies.
    /// </para>
    /// <para>
    /// The benchmark is checked outside of the measured time, so a store that loses data or finds wrong results fails the
    /// run instead of reporting a time: <paramref name="checkBackground"/> must accept the store before the first and after
    /// the last iteration, and one iteration before the measurement must find <see cref="Work{TStore}.Expected"/>.
    /// </para>
    /// </remarks>
    public static Estimate Measure<TStore>(Sampling sampling, TStore store, Work<TStore> work, Action<TStore, int> checkBackground)
        where TStore : IBenched
    {
        var background = Settings.BackgroundLinks;
        store.Fork(background);
        try
        {
            TimeSpan Iteration()
            {
                var started = Stopwatch.GetTimestamp();
                store.Begin();
                work.Operation(store);
                store.Commit();
                var elapsed = Stopwatch.GetElapsedTime(started);

                store.Begin();
                work.Undo(store);
                store.Commit();
                return elapsed;
            }

            checkBackground(store, background);
            var before = Found.Total;
            Iteration();
            var found = Found.Total - before;
            if (found != work.Expected)
            {
                throw new InvalidOperationException($"An iteration found {found} instead of {work.Expected}.");
            }
            checkBackground(store, background);
            var estimate = Measure(sampling, Iteration);
            checkBackground(store, background);
            return estimate;
        }
        finally
        {
            store.Unfork();
        }
    }
}
