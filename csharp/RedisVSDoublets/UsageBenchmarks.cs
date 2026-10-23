namespace Comparisons.RedisVSDoublets;

/// <summary>
/// The operations of the typical Redis usage benchmarks, the same as in <c>rust/benches/benchmarks/usage.rs</c>.
/// They are written once and run unchanged on every store.
/// </summary>
/// <remarks>
/// <c>B</c> is <see cref="Settings.BackgroundLinks"/> and <c>N</c> is <see cref="Settings.BenchmarkLinks"/>.
/// Before every iteration the store contains the keys <c>1..=B</c> (each with its own number as the value),
/// the queue <c>1, ..., B</c> and the set <c>{1, ..., B}</c>.
/// <list type="table">
/// <item><term>KeyValue_Get</term><description><c>N</c> × <c>GET k</c> for the keys <c>1..=N</c></description></item>
/// <item><term>KeyValue_Set</term><description><c>N</c> × <c>SET k k+B</c> for the keys <c>1..=N</c>; undone by <c>SET k k</c></description></item>
/// <item><term>KeyValue_Delete</term><description><c>N</c> × <c>DEL k</c> for the keys <c>B</c> down to <c>B - N + 1</c>; undone by <c>SET k k</c></description></item>
/// <item><term>Counter_Increment</term><description><c>N</c> × <c>INCR k</c> for the keys <c>1..=N</c>; undone by <c>SET k k</c></description></item>
/// <item><term>Queue_Push</term><description><c>N</c> × <c>RPUSH B+i</c>; undone by <c>N</c> × <c>RPOP</c></description></item>
/// <item><term>Queue_Pop</term><description><c>N</c> × <c>LPOP</c>, which returns <c>1..=N</c>; undone by <c>LPUSH</c> of <c>N</c> down to <c>1</c></description></item>
/// <item><term>Set_Add</term><description><c>N</c> × <c>SADD B+i</c>; undone by <c>SREM</c></description></item>
/// <item><term>Set_Remove</term><description><c>N</c> × <c>SREM m</c> for the members <c>B</c> down to <c>B - N + 1</c>; undone by <c>SADD</c></description></item>
/// <item><term>Set_Contains</term><description><c>N</c> × <c>SISMEMBER m</c> for the members <c>1..=N</c></description></item>
/// </list>
/// </remarks>
public static class UsageBenchmarks
{
    /// <summary>The benchmark groups, in the order in which the Rust benchmarks run them.</summary>
    public static readonly string[] Groups =
    [
        "KeyValue_Get", "KeyValue_Set", "KeyValue_Delete", "Counter_Increment",
        "Queue_Push", "Queue_Pop", "Set_Add", "Set_Remove", "Set_Contains",
    ];

    private static void Visit(ulong value) => Benchmarks.Visited += value;

    private static void Visit(bool found) => Benchmarks.Visited += found ? 1UL : 0;

    /// <summary>The work of <paramref name="group"/>.</summary>
    public static Work<IBenchedUsage> Work(string group, int background, int count)
    {
        var (b, n) = ((ulong)background, (ulong)count);
        static void Nothing(IBenchedUsage store)
        {
        }

        void ForFirst(Action<ulong> action)
        {
            for (ulong k = 1; k <= n; k++)
            {
                action(k);
            }
        }

        void ForLast(Action<ulong> action)
        {
            for (var k = b; k > b - n; k--)
            {
                action(k);
            }
        }

        return group switch
        {
            "KeyValue_Get" => new(store => ForFirst(k => store.Get(k, Visit)), Nothing),
            "KeyValue_Set" => new(store => ForFirst(k => store.Set(k, k + b)), store => ForFirst(k => store.Set(k, k))),
            "KeyValue_Delete" => new(store => ForLast(store.Delete), store => ForLast(k => store.Set(k, k))),
            "Counter_Increment" => new(store => ForFirst(store.Increment), store => ForFirst(k => store.Set(k, k))),
            "Queue_Push" => new(store => ForFirst(i => store.Push(b + i)), store => ForFirst(_ => store.PopBack())),
            "Queue_Pop" => new(
                store => ForFirst(_ => store.Pop(Visit)),
                store =>
                {
                    for (var k = n; k >= 1; k--)
                    {
                        store.PushFront(k);
                    }
                }),
            "Set_Add" => new(store => ForFirst(i => store.Add(b + i)), store => ForFirst(i => store.Remove(b + i))),
            "Set_Remove" => new(store => ForLast(store.Remove), store => ForLast(store.Add)),
            "Set_Contains" => new(store => ForFirst(k => store.Contains(k, Visit)), Nothing),
            _ => throw new ArgumentException($"unknown benchmark group {group}", nameof(group)),
        };
    }
}
