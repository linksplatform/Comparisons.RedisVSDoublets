using System;
using System.Collections.Generic;
using System.Linq;
using BenchmarkDotNet.Running;
using Comparisons.RedisVSDoublets.Model;
using Comparisons.RedisVSDoublets.Redis;
using Comparisons.RedisVSDoublets.Doublets;

namespace Comparisons.RedisVSDoublets
{
    public class Program
    {
        public static void Main(string[] args)
        {
            BenchmarkRunner.Run<Benchmarks>();
            
            // Uncomment the line below and comment the line above to run manual tests
            // Run();
        }

        public static void Run()
        {
            Console.WriteLine("Redis vs Doublets Comparison");
            Console.WriteLine("============================");

            var numberOfRecords = new[] { 1000, 10000, 100000 };
            var runs = 3;

            foreach (var recordCount in numberOfRecords)
            {
                Console.WriteLine($"\nTesting with {recordCount} records:");
                Console.WriteLine(new string('-', 40));

                var redisResults = new List<TestRunResults>();
                var doubletsResults = new List<TestRunResults>();

                // Generate test data
                BlogPosts.GenerateData(recordCount);

                // Run Redis tests
                for (int i = 0; i < runs; i++)
                {
                    var redisTestRun = new RedisTestRun("localhost:6379");
                    redisTestRun.Run();
                    redisResults.Add(redisTestRun.Results);
                }

                // Run Doublets tests
                for (int i = 0; i < runs; i++)
                {
                    var doubletsTestRun = new DoubletsTestRun("in-memory");
                    doubletsTestRun.Run();
                    doubletsResults.Add(doubletsTestRun.Results);
                }

                // Calculate and display averages
                DisplayAverageResults("Redis", redisResults);
                DisplayAverageResults("Doublets", doubletsResults);
            }
        }

        private static void DisplayAverageResults(string dbType, List<TestRunResults> results)
        {
            if (results.Count == 0) return;

            var avgPrepareTime = new TimeSpan((long)results.Average(r => r.PrepareTime.Ticks));
            var avgCreationTime = new TimeSpan((long)results.Average(r => r.ListCreationTime.Ticks));
            var avgReadingTime = new TimeSpan((long)results.Average(r => r.ListReadingTime.Ticks));
            var avgDeletionTime = new TimeSpan((long)results.Average(r => r.ListDeletionTime.Ticks));
            var avgDbSizeAfterCreation = (long)results.Average(r => r.DbSizeAfterCreation);

            Console.WriteLine($"\n{dbType} Average Results:");
            Console.WriteLine($"  Prepare time: {avgPrepareTime}");
            Console.WriteLine($"  List creation time: {avgCreationTime}");
            Console.WriteLine($"  List reading time: {avgReadingTime}");
            Console.WriteLine($"  List deletion time: {avgDeletionTime}");
            if (avgDbSizeAfterCreation > 0)
                Console.WriteLine($"  DB size after creation: {avgDbSizeAfterCreation} bytes");
        }
    }
}