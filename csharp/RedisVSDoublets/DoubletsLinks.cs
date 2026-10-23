using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.Split.Generic;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Comparisons.RedisVSDoublets;

/// <summary>
/// An in-memory Doublets store of <see href="https://www.nuget.org/packages/Platform.Data.Doublets">Platform.Data.Doublets</see>.
/// The store runs inside the benchmark process: every operation reads and writes the store's memory directly,
/// without a server, a network connection or a protocol.
/// </summary>
/// <remarks>
/// <list type="table">
/// <item><term><c>Doublets_United_Volatile</c></term><description><c>UnitedMemoryLinks</c> in RAM: a link and its index tree nodes are kept together</description></item>
/// <item><term><c>Doublets_Split_Volatile</c></term><description><c>SplitMemoryLinks</c> in RAM: the links and the index trees are kept in separate memory blocks</description></item>
/// </list>
/// Redis keeps its data in memory, so only the volatile (in-memory) stores are compared, without memory-mapped files.
/// </remarks>
public sealed class DoubletsLinks(ILinks<ulong> links) : IBenchedLinks
{
    public static readonly string[] Variants = ["Doublets_United_Volatile", "Doublets_Split_Volatile"];

    private readonly ulong _continue = links.Constants.Continue;

    public ulong Any { get; } = links.Constants.Any;

    public static DoubletsLinks Open(string variant) => new(variant switch
    {
        "Doublets_United_Volatile" => new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory()),
        "Doublets_Split_Volatile" => new SplitMemoryLinks<ulong>(new HeapResizableDirectMemory(), new HeapResizableDirectMemory()),
        _ => throw new ArgumentException($"unknown Doublets store {variant}", nameof(variant)),
    });

    public void Fork(int background)
    {
        for (var i = 0; i < background; i++)
        {
            links.CreatePoint();
        }
    }

    /// <summary>Deletes the links from the highest id down, so the next created link gets id 1 again.</summary>
    public void Unfork()
    {
        var ids = new List<ulong>();
        Each(Any, Any, Any, link => ids.Add(link.Id));
        foreach (var id in ids.OrderDescending())
        {
            Delete(id);
        }
    }

    public ulong CreatePoint() => links.CreatePoint();

    public void Update(ulong id, ulong source, ulong target) => links.Update(id, source, target);

    /// <summary>Resets the link to <c>(0, 0)</c>, which removes it from the index trees, and then frees it.</summary>
    public void Delete(ulong id) => links.Delete(id, handler: null);

    public void Each(ulong id, ulong source, ulong target, Action<Link> visit) => links.Each(link =>
    {
        visit(new Link(link![0], link[1], link[2]));
        return _continue;
    }, id, source, target);

    public ulong Count() => links.Count();

    public void Dispose() => (links as IDisposable)?.Dispose();
}
