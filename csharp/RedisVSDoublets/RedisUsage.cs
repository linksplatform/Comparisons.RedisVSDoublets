using StackExchange.Redis;

namespace Comparisons.RedisVSDoublets;

/// <summary>
/// The typical Redis data structures (<see cref="IBenchedUsage"/>) in their native Redis types: every operation is one
/// native command.
/// </summary>
/// <remarks>
/// <list type="table">
/// <item><term><c>k:{key}</c></term><description>string: <c>SET</c>, <c>GET</c>, <c>DEL</c>, <c>INCR</c></description></item>
/// <item><term><c>queue</c></term><description>list: <c>RPUSH</c>, <c>LPOP</c>, <c>LPUSH</c>, <c>RPOP</c></description></item>
/// <item><term><c>members</c></term><description>set: <c>SADD</c>, <c>SREM</c>, <c>SISMEMBER</c></description></item>
/// </list>
/// </remarks>
public sealed class RedisUsage : RedisStore, IBenchedUsage
{
    private const string Queue = "queue";
    private const string Members = "members";

    private RedisUsage(string url, Mode mode) : base(url, mode)
    {
    }

    /// <summary>Connects to <paramref name="url"/> and removes all keys of the database.</summary>
    public static RedisUsage Open(Mode mode, string? url = null)
    {
        var store = new RedisUsage(url ?? Url, mode);
        store.Unfork();
        return store;
    }

    private static RedisKey Key(ulong key) => $"k:{key}";

    /// <summary>Creates the background data with <c>MSET</c>, <c>RPUSH</c> and <c>SADD</c>, <see cref="RedisStore.BatchSize"/> values per command.</summary>
    public override void Fork(int background)
    {
        foreach (var (first, last) in Chunks(1, (ulong)background))
        {
            var numbers = Numbers(first, last);
            Database.StringSet(numbers.Select(number => new KeyValuePair<RedisKey, RedisValue>(Key((ulong)number), number)).ToArray());
            Database.ListRightPush(Queue, numbers);
            Database.SetAdd(Members, numbers);
        }
    }

    private static RedisValue[] Numbers(ulong first, ulong last)
    {
        var numbers = new RedisValue[last - first + 1];
        for (var i = 0; i < numbers.Length; i++)
        {
            numbers[i] = first + (ulong)i;
        }
        return numbers;
    }

    public void Set(ulong key, ulong value) => Reply(Commands.StringSetAsync(Key(key), value));

    public void Get(ulong key, Action<ulong> found) => Reply(Commands.StringGetAsync(Key(key)), value =>
    {
        if (!value.IsNull)
        {
            found((ulong)value);
        }
    });

    public void Delete(ulong key) => Reply(Commands.KeyDeleteAsync(Key(key)));

    public void Increment(ulong key) => Reply(Commands.StringIncrementAsync(Key(key)));

    public void Push(ulong value) => Reply(Commands.ListRightPushAsync(Queue, value));

    public void Pop(Action<ulong> popped) => Reply(Commands.ListLeftPopAsync(Queue), value =>
    {
        if (!value.IsNull)
        {
            popped((ulong)value);
        }
    });

    public void PushFront(ulong value) => Reply(Commands.ListLeftPushAsync(Queue, value));

    public void PopBack() => Reply(Commands.ListRightPopAsync(Queue));

    public void Add(ulong member) => Reply(Commands.SetAddAsync(Members, member));

    public void Remove(ulong member) => Reply(Commands.SetRemoveAsync(Members, member));

    public void Contains(ulong member, Action<bool> result) => Reply(Commands.SetContainsAsync(Members, member), result);

    public UsageSnapshot Snapshot()
    {
        var values = new List<(ulong, ulong)>();
        var server = Database.Multiplexer.GetServers()[0];
        foreach (var key in server.Keys(Database.Database, "k:*", pageSize: 1000))
        {
            values.Add((ulong.Parse(((string)key!)[2..]), (ulong)Database.StringGet(key)));
        }
        var queue = Database.ListRange(Queue).Select(value => (ulong)value).ToList();
        var members = Database.SetMembers(Members).Select(value => (ulong)value).Order().ToList();
        return new(values.Order().ToList(), queue, members);
    }
}
