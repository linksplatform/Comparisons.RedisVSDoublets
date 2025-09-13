using BenchmarkDotNet.Attributes;
using BenchmarkDotNet.Configs;
using BenchmarkDotNet.Diagnosers;
using BenchmarkDotNet.Jobs;
using BenchmarkDotNet.Order;
using Comparisons.RedisVSDoublets.Model;
using Comparisons.RedisVSDoublets.Redis;
using Comparisons.RedisVSDoublets.Doublets;

namespace Comparisons.RedisVSDoublets
{
    [Config(typeof(Config))]
    [Orderer(SummaryOrderPolicy.FastestToSlowest)]
    [MemoryDiagnoser]
    public class Benchmarks
    {
        private class Config : ManualConfig
        {
            public Config()
            {
                AddJob(Job.Default);
                AddDiagnoser(MemoryDiagnoser.Default);
            }
        }

        [Params(1000, 10000, 100000)]
        public int NumberOfRecords { get; set; }

        private RedisTestRun? _redisTestRun;
        private DoubletsTestRun? _doubletsTestRun;

        [GlobalSetup]
        public void Setup()
        {
            BlogPosts.GenerateData(NumberOfRecords);
            _redisTestRun = new RedisTestRun("localhost:6379");
            _doubletsTestRun = new DoubletsTestRun("in-memory");
        }

        [Benchmark]
        public void Redis()
        {
            _redisTestRun?.Run();
            LogDatabaseSize("Redis", _redisTestRun?.Results.DbSizeAfterCreation ?? 0);
        }

        [Benchmark]
        public void Doublets()
        {
            _doubletsTestRun?.Run();
            LogDatabaseSize("Doublets", _doubletsTestRun?.Results.DbSizeAfterCreation ?? 0);
        }

        [GlobalCleanup]
        public void Cleanup()
        {
            _redisTestRun?.DeleteDatabase();
            _doubletsTestRun?.DeleteDatabase();
        }

        private static void LogDatabaseSize(string dbType, long size)
        {
            Console.WriteLine($"{dbType} database size: {size} bytes");
        }
    }
}