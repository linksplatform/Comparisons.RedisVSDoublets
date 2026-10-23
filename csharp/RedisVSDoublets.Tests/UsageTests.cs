using Xunit;

namespace Comparisons.RedisVSDoublets.Tests;

/// <summary>
/// Checks that the Doublets reimplementation of the typical Redis data structures behaves as Redis does, and that the
/// usage benchmarks leave the store as they found it. The same checks as in <c>rust/tests/same_behavior.rs</c>.
/// </summary>
public class UsageTests
{
    private const int Background = 20;
    private const int Count = 5;

    private static void InTransaction(IBenched store, Action action)
    {
        store.Begin();
        action();
        store.Commit();
    }

    /// <summary>Runs the commands of the Redis documentation for every structure, each in <c>Begin</c>/<c>Commit</c>.</summary>
    private static void Check(IBenchedUsage store)
    {
        store.Fork(0);
        Assert.Equal(UsageSnapshot.Background(0), store.Snapshot());

        // Key-value: SET creates or replaces, GET of a missing key finds nothing, INCR counts a missing key from 0, DEL is idempotent.
        var values = new List<ulong>();
        InTransaction(store, () =>
        {
            store.Get(1, values.Add);
            store.Set(1, 10);
            store.Get(1, values.Add);
            store.Set(1, 11);
            store.Get(1, values.Add);
            store.Increment(1);
            store.Increment(2);
            store.Increment(2);
            store.Get(2, values.Add);
            store.Set(3, 0);
            store.Delete(3);
            store.Delete(3);
            store.Get(3, values.Add);
            store.Delete(4);
        });
        Assert.Equal([10UL, 11, 2], values);
        Assert.Equal([(1UL, 12UL), (2, 2)], store.Snapshot().Values);

        // Queue: FIFO, with LPUSH and RPOP at the other ends; LPOP of an empty queue finds nothing.
        var popped = new List<ulong>();
        InTransaction(store, () =>
        {
            store.Pop(popped.Add);
            store.PopBack();
            store.Push(1);
            store.Push(2);
            store.Push(3);
            store.Pop(popped.Add);
            store.PushFront(7);
            store.Push(4);
            store.PopBack();
            store.Push(2);
        });
        Assert.Equal([1UL], popped);
        Assert.Equal([7UL, 2, 3, 2], store.Snapshot().Queue);
        InTransaction(store, () =>
        {
            for (var i = 0; i < 5; i++)
            {
                store.Pop(popped.Add);
            }
            store.PopBack();
            store.Push(5);
            store.PopBack();
            store.PushFront(6);
        });
        Assert.Equal([1UL, 7, 2, 3, 2], popped);
        Assert.Equal([6UL], store.Snapshot().Queue);

        // Set: SADD and SREM are idempotent.
        var contained = new List<bool>();
        InTransaction(store, () =>
        {
            store.Contains(1, contained.Add);
            store.Add(1);
            store.Add(1);
            store.Add(2);
            store.Contains(1, contained.Add);
            store.Remove(1);
            store.Remove(1);
            store.Remove(3);
            store.Contains(1, contained.Add);
            store.Contains(2, contained.Add);
        });
        Assert.Equal([false, true, false, true], contained);
        Assert.Equal([2UL], store.Snapshot().Members);

        // Keys, queue values and members are independent: the same number in all of them.
        InTransaction(store, () =>
        {
            store.Set(2, 2);
            store.Push(2);
            store.Add(6);
            store.Set(6, 2);
        });
        Assert.Equal(new UsageSnapshot([(1, 12), (2, 2), (6, 2)], [6, 2], [2, 6]), store.Snapshot());

        store.Unfork();
        Assert.Equal(UsageSnapshot.Background(0), store.Snapshot());
        store.Fork(Background);
        Assert.Equal(UsageSnapshot.Background(Background), store.Snapshot());
        store.Unfork();
    }

    /// <summary>Runs the work of every usage benchmark and checks its effect and that its undo restores the background data.</summary>
    private static void CheckUndone(IBenchedUsage store)
    {
        var background = UsageSnapshot.Background(Background);
        store.Fork(Background);
        foreach (var group in UsageBenchmarks.Groups)
        {
            var work = UsageBenchmarks.Work(group, Background, Count);
            for (var iteration = 0; iteration < 3; iteration++)
            {
                var visited = Benchmarks.Visited;
                InTransaction(store, () => work.Operation(store));
                var changed = store.Snapshot();
                Assert.Equal(Expected(group), changed);
                Assert.Equal(VisitedSum(group), Benchmarks.Visited - visited);
                InTransaction(store, () => work.Undo(store));
                Assert.Equal(background, store.Snapshot());
            }
        }
        store.Unfork();
    }

    /// <summary>The data after the work of <paramref name="group"/>, computed with lists.</summary>
    private static UsageSnapshot Expected(string group)
    {
        var (values, queue, members) = (UsageSnapshot.Background(Background).Values.ToList(),
            UsageSnapshot.Background(Background).Queue.ToList(), UsageSnapshot.Background(Background).Members.ToList());
        var (b, n) = ((ulong)Background, (ulong)Count);
        switch (group)
        {
            case "KeyValue_Set":
                values = values.Select(pair => pair.Key <= n ? (pair.Key, pair.Key + b) : pair).ToList();
                break;
            case "KeyValue_Delete":
                values.RemoveAll(pair => pair.Key > b - n);
                break;
            case "Counter_Increment":
                values = values.Select(pair => pair.Key <= n ? (pair.Key, pair.Value + 1) : pair).ToList();
                break;
            case "Queue_Push":
                queue.AddRange(Enumerable.Range(1, Count).Select(i => b + (ulong)i));
                break;
            case "Queue_Pop":
                queue.RemoveRange(0, Count);
                break;
            case "Set_Add":
                members.AddRange(Enumerable.Range(1, Count).Select(i => b + (ulong)i));
                break;
            case "Set_Remove":
                members.RemoveAll(member => member > b - n);
                break;
        }
        return new(values, queue, members);
    }

    /// <summary>What the reads of <paramref name="group"/> add to <see cref="Benchmarks.Visited"/>.</summary>
    private static ulong VisitedSum(string group) => group switch
    {
        // The keys 1..=N have their own numbers as values, the queue starts with 1..=N.
        "KeyValue_Get" or "Queue_Pop" => (ulong)(Count * (Count + 1) / 2),
        "Set_Contains" => Count,
        _ => 0,
    };

    [Fact]
    public void Doublets()
    {
        using var store = new DoubletsUsage();
        Check(store);
    }

    [Fact]
    public void DoubletsBenchmarkWorkIsUndone()
    {
        using var store = new DoubletsUsage();
        CheckUndone(store);
    }

    /// <summary>Doublets requires every <c>(source, target)</c> pair to be unique; a long queue of repeated values must still work.</summary>
    [Fact]
    public void DoubletsQueueKeepsRepeatedValues()
    {
        using var store = new DoubletsUsage();
        store.Fork(0);
        var expected = new LinkedList<ulong>();
        var random = new Random(3);
        for (var step = 0; step < 2000; step++)
        {
            var value = (ulong)random.Next(1, 4);
            switch (random.Next(4))
            {
                case 0:
                    store.Push(value);
                    expected.AddLast(value);
                    break;
                case 1:
                    store.PushFront(value);
                    expected.AddFirst(value);
                    break;
                case 2:
                    var popped = new List<ulong>();
                    store.Pop(popped.Add);
                    Assert.Equal(expected.Count > 0 ? [expected.First!.Value] : [], popped);
                    if (expected.Count > 0)
                    {
                        expected.RemoveFirst();
                    }
                    break;
                default:
                    store.PopBack();
                    if (expected.Count > 0)
                    {
                        expected.RemoveLast();
                    }
                    break;
            }
            Assert.Equal(expected, store.Snapshot().Queue);
        }
    }

    /// <summary>All modes share one database, so they run one after another in one test.</summary>
    [Fact]
    public void Redis()
    {
        SameBehaviorTests.SkipWithoutRedis();
        foreach (var mode in SameBehaviorTests.Modes)
        {
            using var store = RedisUsage.Open(mode);
            Check(store);
        }
    }

    [Fact]
    public void RedisBenchmarkWorkIsUndone()
    {
        SameBehaviorTests.SkipWithoutRedis();
        foreach (var mode in SameBehaviorTests.Modes)
        {
            using var store = RedisUsage.Open(mode);
            CheckUndone(store);
        }
    }
}
