using Comparisons.RedisVSDoublets;
using Xunit;

namespace RedisVSDoublets.Tests;

public sealed class HarnessTests
{
    [Fact]
    public void EvenMedianAndSampleDeviation()
    {
        var estimate = Estimate.Of([4, 1, 3, 2]);
        Assert.Equal(2.5, estimate.Median);
        Assert.Equal(Math.Sqrt(5.0 / 3), estimate.StandardDeviation, precision: 10);
    }

    [Fact]
    public void OddMedianAndOutput()
    {
        var estimate = Estimate.Of([5, 1, 3]);
        Assert.Equal(3, estimate.Median);
        Assert.Equal(2, estimate.StandardDeviation);
        Assert.Equal("test Create/Redis ... bench: 3 ns/iter (+/- 2)", estimate.Bencher("Create", "Redis"));
    }
}
