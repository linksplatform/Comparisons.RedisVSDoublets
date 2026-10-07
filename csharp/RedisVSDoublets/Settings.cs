using System.Globalization;

namespace Comparisons.RedisVSDoublets;

public sealed record Settings(string Backend, int Background, int Links, int Samples)
{
    private static int Read(string name, int fallback) => Environment.GetEnvironmentVariable(name) is { } value
        ? int.Parse(value, CultureInfo.InvariantCulture) : fallback;

    public static Settings FromEnvironment()
    {
        var settings = new Settings(
            Environment.GetEnvironmentVariable("BENCHMARK_BACKEND") ?? "doublets",
            Read("BENCHMARK_BACKGROUND_LINKS", 1000), Read("BENCHMARK_LINKS", 100), Read("BENCHMARK_SAMPLES", 10));
        if (settings.Links <= 0 || settings.Links > settings.Background || settings.Samples < 3 || settings.Background > 1040383 - settings.Links)
            throw new ArgumentException("require 0 < links <= background, background + links <= 1040383, samples >= 3");
        return settings;
    }
}
