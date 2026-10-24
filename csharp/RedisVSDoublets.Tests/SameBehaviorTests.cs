using Xunit;

namespace Comparisons.RedisVSDoublets.Tests;

/// <summary>
/// Checks that every benchmarked links store gives the same results for the operations used by the benchmarks,
/// and that undoing an operation (as the benchmarks do after every iteration) restores the background links.
/// The same checks as in <c>rust/tests/same_behavior.rs</c>.
/// </summary>
/// <remarks>
/// The Redis tests need a running server, whose database they empty, and are skipped unless <c>REDIS_URL</c> is set
/// (for example <c>REDIS_URL=redis://localhost:6379 dotnet test</c>).
/// </remarks>
public class SameBehaviorTests
{
    private const int Background = 20;
    private const int Links = 5;

    public static readonly Mode[] Modes = [Mode.NonTransaction, Mode.Transaction, Mode.Pipeline];

    internal static void SkipWithoutRedis() =>
        Assert.SkipWhen(string.IsNullOrEmpty(Environment.GetEnvironmentVariable("REDIS_URL")), "REDIS_URL is not set");

    private static List<Link> Query(IBenchedLinks store, ulong id, ulong source, ulong target)
    {
        var links = new List<Link>();
        store.Each(id, source, target, links.Add);
        return links.OrderBy(link => link.Id).ToList();
    }

    private static List<Link> All(IBenchedLinks store) => Query(store, store.Any, store.Any, store.Any);

    private static List<Link> Points(int first, int last) =>
        Enumerable.Range(first, last - first + 1).Select(id => Link.Point((ulong)id)).ToList();

    private static void InTransaction(IBenched store, Action action)
    {
        store.Begin();
        action();
        store.Commit();
    }

    /// <summary>
    /// Runs the operations of the benchmarks inside <c>Begin</c>/<c>Commit</c> and checks their results. Redis in
    /// transaction and pipeline modes passes the found links to the callbacks in <c>Commit</c>, so they are checked after it.
    /// </summary>
    private static void Check(IBenchedLinks store)
    {
        var any = store.Any;
        store.Fork(Background);
        var background = Points(1, Background);
        Assert.Equal(background, All(store));

        // Create, undone by Delete.
        var created = new List<ulong>();
        InTransaction(store, () =>
        {
            for (var i = 0; i < Links; i++)
            {
                created.Add(store.CreatePoint());
            }
        });
        Assert.Equal(Points(Background + 1, Background + Links).Select(link => link.Id), created);
        Assert.Equal(Points(1, Background + Links), All(store));
        InTransaction(store, () =>
        {
            for (var id = (ulong)(Background + Links); id > Background; id--)
            {
                store.Delete(id);
            }
        });
        Assert.Equal(background, All(store));

        // Delete, undone by Create.
        InTransaction(store, () =>
        {
            for (var id = (ulong)Background; id > Background - Links; id--)
            {
                store.Delete(id);
            }
        });
        Assert.Equal(Points(1, Background - Links), All(store));
        InTransaction(store, () =>
        {
            for (var i = 0; i < Links; i++)
            {
                store.CreatePoint();
            }
        });
        Assert.Equal(background, All(store));

        // Update, restored by the second update; the query between them sees the first one.
        var updated = new List<Link>();
        InTransaction(store, () =>
        {
            store.Update(Background, 0, 0);
            store.Each(Background, any, any, updated.Add);
            store.Update(Background, Background, Background);
        });
        Assert.Equal([new Link(Background, 0, 0)], updated);
        Assert.Equal(background, All(store));

        // The queries of the benchmarks, inside Begin/Commit as in the benchmarks.
        var found = new List<(string Query, List<Link> Links)>();
        InTransaction(store, () =>
        {
            for (ulong id = 1; id <= Background; id++)
            {
                foreach (var (name, query) in new (string, (ulong, ulong, ulong))[]
                {
                    ("identity", (id, any, any)), ("concrete", (any, id, id)), ("outgoing", (any, id, any)), ("incoming", (any, any, id)),
                })
                {
                    var links = new List<Link>();
                    found.Add(($"{name} {id}", links));
                    store.Each(query.Item1, query.Item2, query.Item3, links.Add);
                }
            }
        });
        Assert.Equal(Background * 4, found.Count);
        foreach (var (query, links) in found)
        {
            var id = ulong.Parse(query.Split(' ')[1]);
            Assert.True(links.SequenceEqual([Link.Point(id)]), query);
        }
        Assert.Equal((ulong)Background, store.Count());

        // The benchmarks run every iteration on a store that `Unfork` emptied and `Fork` filled again.
        store.Unfork();
        Assert.Equal(0UL, store.Count());
        store.Fork(Background);
        Assert.Equal(background, All(store));
        store.Unfork();
    }

    [Theory]
    [InlineData("Doublets_United_Volatile")]
    [InlineData("Doublets_Split_Volatile")]
    public void Doublets(string variant)
    {
        using var store = DoubletsLinks.Open(variant);
        Check(store);
    }

    [Fact]
    public void DoubletsVariantsAreAllTested() =>
        Assert.Equal(DoubletsLinks.Variants, typeof(SameBehaviorTests).GetMethod(nameof(Doublets))!
            .GetCustomAttributes(typeof(InlineDataAttribute), false)
            .Select(attribute => (string)((InlineDataAttribute)attribute).Data[0]!));

    /// <summary>All modes share one database, so they run one after another in one test.</summary>
    [Fact]
    public void Redis()
    {
        SkipWithoutRedis();
        foreach (var mode in Modes)
        {
            using var store = RedisLinks.Open(mode);
            Check(store);
        }
    }

    /// <summary>
    /// Runs the same random changes on Redis and on the United Doublets store, including links whose source and target
    /// differ, and checks that every query shape finds the same links in both. Every <c>(source, target)</c> pair is kept
    /// unique, as Doublets requires.
    /// </summary>
    [Fact]
    public void RedisFindsTheSameLinksAsDoublets()
    {
        SkipWithoutRedis();
        using var redis = RedisLinks.Open(Mode.NonTransaction);
        using var doublets = DoubletsLinks.Open("Doublets_United_Volatile");
        var random = new Random(7);
        var pairs = new Dictionary<ulong, (ulong Source, ulong Target)>();
        foreach (var store in new IBenchedLinks[] { redis, doublets })
        {
            store.Fork(Background);
        }
        for (ulong id = 1; id <= Background; id++)
        {
            pairs[id] = (id, id);
        }
        for (var step = 0; step < 200; step++)
        {
            var id = (ulong)random.Next(1, Background + 1);
            var pair = ((ulong)random.Next(1, Background + 1), (ulong)random.Next(1, Background + 1));
            if (pairs.ContainsValue(pair))
            {
                continue;
            }
            pairs[id] = pair;
            redis.Update(id, pair.Item1, pair.Item2);
            doublets.Update(id, pair.Item1, pair.Item2);
        }
        var any = redis.Any;
        Assert.Equal(All(doublets), All(redis));
        Assert.Equal(pairs.Select(pair => new Link(pair.Key, pair.Value.Source, pair.Value.Target)).OrderBy(link => link.Id), All(redis));
        for (ulong value = 1; value <= Background; value++)
        {
            foreach (var (id, source, target) in new[]
            {
                (value, any, any), (any, value, any), (any, any, value), (any, value, value),
                (any, value, (value % Background) + 1), (value, pairs[value].Source, any), (value, any, value),
            })
            {
                Assert.Equal(Query(doublets, id, source, target), Query(redis, id, source, target));
            }
        }
    }

    /// <summary>A change of a missing link is an error in every mode, reported at the latest by <c>Commit</c>, not silently skipped.</summary>
    [Fact]
    public void RedisReportsErrorsInEveryMode()
    {
        SkipWithoutRedis();
        foreach (var mode in Modes)
        {
            using var store = RedisLinks.Open(mode);
            store.Fork(2);
            foreach (var change in new Action[] { () => store.Update(3, 1, 1), () => store.Delete(3) })
            {
                var error = Assert.ThrowsAny<Exception>(() => InTransaction(store, change));
                Assert.Contains("link 3 does not exist", error.Message);
            }
            Assert.Equal(Points(1, 2), All(store));
            store.Unfork();
        }
    }

    [Theory]
    [InlineData("redis://localhost", "localhost", 6379, null, null, null)]
    [InlineData("redis://db.example:6380/2", "db.example", 6380, null, null, 2)]
    [InlineData("redis://:secret@localhost:6379/0", "localhost", 6379, null, "secret", 0)]
    [InlineData("redis://user:p%40ss@localhost", "localhost", 6379, "user", "p@ss", null)]
    public void RedisUrlIsReadAsByTheRustClient(string url, string host, int port, string? user, string? password, int? database)
    {
        var options = RedisStore.Options(url);
        var endpoint = Assert.IsType<System.Net.DnsEndPoint>(Assert.Single(options.EndPoints));
        Assert.Equal((host, port), (endpoint.Host, endpoint.Port));
        Assert.Equal(user, options.User);
        Assert.Equal(password, options.Password);
        Assert.Equal(database, options.DefaultDatabase);
    }

    /// <summary>Runs every benchmark's work, as the benchmark does, and checks that the store ends as it started.</summary>
    [Fact]
    public void BenchmarkWorkIsUndone()
    {
        foreach (var group in Benchmarks.Groups)
        {
            foreach (var variant in DoubletsLinks.Variants)
            {
                using var store = DoubletsLinks.Open(variant);
                CheckUndone(store, group);
            }
        }
    }

    [Fact]
    public void RedisBenchmarkWorkIsUndone()
    {
        SkipWithoutRedis();
        foreach (var mode in Modes)
        {
            using var store = RedisLinks.Open(mode);
            foreach (var group in Benchmarks.Groups)
            {
                CheckUndone(store, group);
            }
        }
    }

    private static void CheckUndone(IBenchedLinks store, string group)
    {
        store.Fork(Background);
        var work = Benchmarks.Single(group, Background, Links);
        for (var iteration = 0; iteration < 3; iteration++)
        {
            InTransaction(store, () => work.Operation(store));
            InTransaction(store, () => work.Undo(store));
            Assert.Equal(Points(1, Background), All(store));
        }
        store.Unfork();
    }
}
