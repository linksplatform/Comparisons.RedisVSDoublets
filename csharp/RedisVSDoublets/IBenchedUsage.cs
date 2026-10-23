namespace Comparisons.RedisVSDoublets;

/// <summary>
/// The data structures of typical Redis usage: a key-value cache with counters, a FIFO queue (a Redis list used as a
/// job queue) and a set. Redis keeps them in its native types; Doublets reimplements them in links
/// (see <see cref="DoubletsUsage"/>). Keys, values and members are numbers.
/// </summary>
/// <remarks>
/// Results are passed to callbacks, because Redis in transaction and pipeline modes receives them only in
/// <see cref="IBenched.Commit"/>.
/// </remarks>
public interface IBenchedUsage : IBenched
{
    /// <summary>
    /// Fills the empty store with the background data: the keys <c>1..=background</c>, each with its own number as the
    /// value, the queue <c>1, 2, ..., background</c> and the set <c>{1, ..., background}</c>.
    /// </summary>
    new void Fork(int background);

    /// <summary>Sets the value of <paramref name="key"/>, creating the key if it does not exist (Redis <c>SET</c>).</summary>
    void Set(ulong key, ulong value);

    /// <summary>Passes the value of <paramref name="key"/> to <paramref name="found"/> if the key exists (Redis <c>GET</c>).</summary>
    void Get(ulong key, Action<ulong> found);

    /// <summary>Deletes <paramref name="key"/> if it exists (Redis <c>DEL</c>).</summary>
    void Delete(ulong key);

    /// <summary>Adds one to the value of <paramref name="key"/>; a missing key counts from 0 (Redis <c>INCR</c>).</summary>
    void Increment(ulong key);

    /// <summary>Appends <paramref name="value"/> to the end of the queue (Redis <c>RPUSH</c>).</summary>
    void Push(ulong value);

    /// <summary>Removes the first value of the queue and passes it to <paramref name="popped"/>, if the queue is not empty (Redis <c>LPOP</c>).</summary>
    void Pop(Action<ulong> popped);

    /// <summary>Inserts <paramref name="value"/> at the start of the queue (Redis <c>LPUSH</c>). Used to undo <see cref="Pop"/>.</summary>
    void PushFront(ulong value);

    /// <summary>Removes the last value of the queue, if any (Redis <c>RPOP</c>). Used to undo <see cref="Push"/>.</summary>
    void PopBack();

    /// <summary>Adds <paramref name="member"/> to the set if it is not there yet (Redis <c>SADD</c>).</summary>
    void Add(ulong member);

    /// <summary>Removes <paramref name="member"/> from the set if it is there (Redis <c>SREM</c>).</summary>
    void Remove(ulong member);

    /// <summary>Passes whether the set contains <paramref name="member"/> to <paramref name="result"/> (Redis <c>SISMEMBER</c>).</summary>
    void Contains(ulong member, Action<bool> result);

    /// <summary>
    /// Returns all data: the keys with their values sorted by key, the queue from its start and the set members sorted.
    /// Not benchmarked; it must not be called between <c>Begin</c> and <c>Commit</c>.
    /// </summary>
    UsageSnapshot Snapshot();

    void IBenched.Fork(int background) => Fork(background);
}

/// <summary>All data of an <see cref="IBenchedUsage"/> store, for the tests.</summary>
public sealed record UsageSnapshot(IReadOnlyList<(ulong Key, ulong Value)> Values, IReadOnlyList<ulong> Queue, IReadOnlyList<ulong> Members)
{
    /// <summary>The data that <see cref="IBenchedUsage.Fork"/> creates.</summary>
    public static UsageSnapshot Background(int background)
    {
        var numbers = Enumerable.Range(1, background).Select(number => (ulong)number).ToList();
        return new(numbers.Select(number => (number, number)).ToList(), numbers, numbers);
    }

    public bool Equals(UsageSnapshot? other) =>
        other is not null && Values.SequenceEqual(other.Values) && Queue.SequenceEqual(other.Queue) && Members.SequenceEqual(other.Members);

    public override int GetHashCode() => HashCode.Combine(Values.Count, Queue.Count, Members.Count);

    public override string ToString() =>
        $"values [{string.Join(", ", Values)}], queue [{string.Join(", ", Queue)}], members [{string.Join(", ", Members)}]";
}
