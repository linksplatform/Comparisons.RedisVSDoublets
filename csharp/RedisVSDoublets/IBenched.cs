namespace Comparisons.RedisVSDoublets;

/// <summary>
/// A store that can be benchmarked: a links store (<see cref="IBenchedLinks"/>) or a store of the typical Redis
/// data structures (<see cref="IBenchedUsage"/>).
/// </summary>
/// <remarks>
/// A benchmark of one operation is:
/// <list type="number">
/// <item><see cref="Fork"/> - not measured: create the background data</item>
/// <item>for every iteration: <see cref="Begin"/>, the operation, <see cref="Commit"/> - measured;
/// then undo the changes of the operation - not measured</item>
/// <item><see cref="Unfork"/> - not measured: remove all data</item>
/// </list>
/// </remarks>
public interface IBenched : IDisposable
{
    /// <summary>Creates the background data in the empty store.</summary>
    void Fork(int background);

    /// <summary>
    /// Starts collecting the commands of one iteration. Only Redis in transaction and pipeline modes collects them;
    /// all other stores run every operation when it is called.
    /// </summary>
    void Begin()
    {
    }

    /// <summary>
    /// Runs the commands collected since <see cref="Begin"/> and waits for all their replies, which are passed to
    /// the callbacks of the operations. For all other stores every operation is already complete when it returns.
    /// </summary>
    void Commit()
    {
    }

    /// <summary>Removes all data.</summary>
    void Unfork();
}
