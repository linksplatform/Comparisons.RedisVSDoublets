using Comparisons.RedisVSDoublets;
using Xunit;

namespace RedisVSDoublets.Tests;

public sealed class SameBehaviorTests
{
    private static Link[] Snapshot(IStore store, ulong id = ulong.MaxValue, ulong source = ulong.MaxValue, ulong target = ulong.MaxValue)
    {
        var found = new List<Link>();
        store.Each(id, source, target, found.Add);
        return found.OrderBy(link => link.Id).ToArray();
    }

    private static void Check(IStore store)
    {
        for (ulong id = 1; id <= 20; id++) Assert.Equal(id, store.Create());
        var points = Enumerable.Range(1, 20).Select(id => new Link((ulong)id, (ulong)id, (ulong)id)).ToArray();
        Assert.Equal(points, Snapshot(store));
        for (var operation = 0; operation < 8; operation++)
        {
            Workload.Exercise(store, operation, 20, 5);
            var expected = operation switch
            {
                0 => Enumerable.Range(1, 25).Select(id => new Link((ulong)id, (ulong)id, (ulong)id)).ToArray(),
                1 => points.Select(link => link.Id <= 5 ? new Link(link.Id, link.Id, link.Id % 20 + 1) : link).ToArray(),
                2 => points.Take(15).ToArray(),
                _ => points,
            };
            Assert.Equal(expected, Snapshot(store));
            Workload.Undo(store, operation, 20, 5);
            Assert.Equal(points, Snapshot(store));
        }
        store.Update(19, 1, 2);
        store.Update(20, 1, 3);
        Assert.Equal([new Link(19, 1, 2)], Snapshot(store, id: 19));
        Assert.Equal([new Link(19, 1, 2)], Snapshot(store, source: 1, target: 2));
        Assert.Equal([new Link(1, 1, 1), new Link(19, 1, 2), new Link(20, 1, 3)], Snapshot(store, source: 1));
        Assert.Equal([new Link(2, 2, 2), new Link(19, 1, 2)], Snapshot(store, target: 2));
        store.Update(19, 4, 5);
        Assert.Empty(Snapshot(store, source: 1, target: 2));
        store.Delete(20);
        Assert.Empty(Snapshot(store, id: 20));
        Assert.Empty(Snapshot(store, source: 1, target: 3));
        Assert.ThrowsAny<Exception>(() => store.Update(99, 1, 2));
        Assert.ThrowsAny<Exception>(() => store.Delete(99));
        Assert.Empty(Snapshot(store, source: 99));
    }

    [Fact]
    public void DoubletsBehavior()
    {
        using var store = new DoubletsStore();
        Check(store);
    }

    [Fact(Skip = "requires REDIS_URL", SkipUnless = nameof(HasRedis), SkipType = typeof(SameBehaviorTests))]
    public void RedisBehaviorAndIsolation()
    {
        using var first = new RedisStore();
        using var second = new RedisStore();
        Assert.False(string.IsNullOrWhiteSpace(first.Version));
        Check(first);
        Assert.Empty(Snapshot(second));
        Assert.Equal(1UL, second.Create());
        first.Dispose();
        Assert.Equal([new Link(1, 1, 1)], Snapshot(second));
    }

    public static bool HasRedis => Environment.GetEnvironmentVariable("REDIS_URL") is not null;
}
