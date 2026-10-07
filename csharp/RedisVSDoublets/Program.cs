namespace Comparisons.RedisVSDoublets;

public static class Program
{
    public static void Main()
    {
        var settings = Settings.FromEnvironment();
        using IStore store = settings.Backend switch
        {
            "redis" => new RedisStore(),
            "doublets" => new DoubletsStore(),
            _ => throw new ArgumentException("BENCHMARK_BACKEND must be redis or doublets"),
        };
        if (store is RedisStore redis) Console.WriteLine($"# redis: {redis.Version}");
        for (var i = 0; i < settings.Background; i++) store.Create();
        var variant = settings.Backend == "redis" ? "Redis" : "Doublets";
        for (var operation = 0; operation < Workload.Operations.Length; operation++)
        {
            var estimate = Harness.Measure(store, operation, settings);
            Console.WriteLine(estimate.Bencher(Workload.Operations[operation], variant));
        }
    }
}
