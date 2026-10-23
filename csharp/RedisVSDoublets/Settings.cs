namespace Comparisons.RedisVSDoublets;

/// <summary>The sizes of a benchmark run, read from the same environment variables as the Rust benchmarks.</summary>
public static class Settings
{
    /// <summary>
    /// Number of point links (and of keys, list items and set members in the Redis usage benchmarks)
    /// that exist before the measured operations start (<c>BENCHMARK_BACKGROUND_LINKS</c>, default 1000).
    /// </summary>
    public static int BackgroundLinks => Number("BENCHMARK_BACKGROUND_LINKS", 1000);

    /// <summary>
    /// Number of links that each benchmark iteration creates, updates, deletes or looks up
    /// (<c>BENCHMARK_LINKS</c>, default 100). Update and Delete work on existing background links,
    /// so this value must not exceed <see cref="BackgroundLinks"/>.
    /// </summary>
    public static int BenchmarkLinks
    {
        get
        {
            var links = Number("BENCHMARK_LINKS", 100);
            if (links > BackgroundLinks)
            {
                throw new InvalidOperationException(
                    $"BENCHMARK_LINKS ({links}) must not exceed BENCHMARK_BACKGROUND_LINKS ({BackgroundLinks})");
            }
            return links;
        }
    }

    /// <summary>The backend to benchmark (<c>BENCHMARK_BACKEND</c>): <c>redis</c>, <c>doublets</c> or both when empty.</summary>
    public static string Backend => Environment.GetEnvironmentVariable("BENCHMARK_BACKEND") ?? "";

    public static string Variable(string name, string fallback) =>
        Environment.GetEnvironmentVariable(name) is { Length: > 0 } value ? value : fallback;

    private static int Number(string name, int fallback)
    {
        var value = Environment.GetEnvironmentVariable(name);
        if (string.IsNullOrEmpty(value))
        {
            return fallback;
        }
        return int.TryParse(value, out var number) && number >= 0
            ? number
            : throw new InvalidOperationException($"{name} must be a non-negative integer, got `{value}`");
    }
}
