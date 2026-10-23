using Xunit;

namespace Comparisons.RedisVSDoublets.Tests;

/// <summary>Checks that the harness samples and reports as Criterion 0.8 does, so the report reads both languages alike.</summary>
public class HarnessTests
{
    [Fact]
    public void FlatSamplingGivesEverySampleTheSameIterations()
    {
        // 5 s over 10 samples of 2 ms iterations: 250 iterations each.
        var counts = Sampling.Redis.IterationCounts(2_000_000);
        Assert.Equal(Enumerable.Repeat(250L, 10), counts);
    }

    [Fact]
    public void LinearSamplingGrowsTheIterationsBySample()
    {
        // 2 s over 1 + 2 + ... + 100 = 5050 runs of 1 µs iterations: d = ceil(396.04) = 397.
        var counts = Sampling.Doublets.IterationCounts(1_000);
        Assert.Equal(100, counts.Length);
        Assert.Equal(397, counts[0]);
        Assert.Equal(397 * 100, counts[^1]);
    }

    [Fact]
    public void SlowIterationsStillRunOncePerSample()
    {
        Assert.Equal(Enumerable.Repeat(1L, 10), Sampling.Redis.IterationCounts(10e9));
        Assert.Equal(Enumerable.Range(1, 100).Select(i => (long)i), Sampling.Doublets.IterationCounts(10e9));
    }

    [Fact]
    public void EstimateIsTheMedianAndTheSampleStandardDeviation()
    {
        Assert.Equal(new Estimate(3, Math.Sqrt(2.5)), Estimate.Of([5, 1, 4, 2, 3]));
        Assert.Equal(2.5, Estimate.Of([4, 1, 3, 2]).Median);
        Assert.Equal(0, Estimate.Of([7]).StandardDeviation);
    }

    [Fact]
    public void BencherLineMatchesCriterion() =>
        Assert.Equal(
            "test Create/Redis_Transaction ... bench:     244,055 ns/iter (+/- 5,345)",
            new Estimate(244_054.6, 5_344.7).Bencher("Create", "Redis_Transaction"));

    [Fact]
    public void MeasureExcludesTheUndoAndTheBackgroundLinks()
    {
        var sampling = new Sampling(3, true, TimeSpan.FromMilliseconds(5), TimeSpan.FromMilliseconds(30));
        using var store = DoubletsLinks.Open("Doublets_United_Volatile");
        var operations = 0;
        var estimate = Harness.Measure(sampling, store,
            links => { operations++; Thread.Sleep(1); },
            links => Thread.Sleep(20));
        Assert.InRange(estimate.Median, 1e6, 15e6);
        Assert.True(operations > 3);
        Assert.Equal(0UL, store.Count());
    }
}
