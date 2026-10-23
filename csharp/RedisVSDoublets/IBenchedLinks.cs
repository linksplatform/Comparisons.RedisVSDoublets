namespace Comparisons.RedisVSDoublets;

/// <summary>A link: its id, the link it starts from (source) and the link it ends at (target).</summary>
public readonly record struct Link(ulong Id, ulong Source, ulong Target)
{
    public static Link Point(ulong id) => new(id, id, id);
}

/// <summary>
/// A links store that can be benchmarked. Redis and every Doublets store implement it,
/// so every benchmark calls exactly the same methods on both databases.
/// </summary>
/// <remarks>
/// Results (the id of <see cref="CreatePoint"/> aside) are passed to callbacks, because Redis in transaction and
/// pipeline modes receives them only in <see cref="IBenched.Commit"/>.
/// </remarks>
public interface IBenchedLinks : IBenched
{
    /// <summary>The value of a query part that matches every value.</summary>
    ulong Any { get; }

    /// <summary>Creates the point links <c>1..=background</c> (each with <c>id = source = target</c>) in the empty store.</summary>
    new void Fork(int background);

    /// <summary>Creates a link with <c>id = source = target</c> and returns its id.</summary>
    ulong CreatePoint();

    /// <summary>Sets the source and the target of the link <paramref name="id"/>.</summary>
    void Update(ulong id, ulong source, ulong target);

    /// <summary>Deletes the link <paramref name="id"/>.</summary>
    void Delete(ulong id);

    /// <summary>Passes every link that matches the query to <paramref name="visit"/>; <see cref="Any"/> matches every value.</summary>
    void Each(ulong id, ulong source, ulong target, Action<Link> visit);

    /// <summary>Returns the number of links. Not benchmarked; it must not be called between <c>Begin</c> and <c>Commit</c>.</summary>
    ulong Count();

    void IBenched.Fork(int background) => Fork(background);
}
