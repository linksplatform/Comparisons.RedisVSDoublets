using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;

namespace Comparisons.RedisVSDoublets;

/// <summary>
/// The typical Redis data structures (<see cref="IBenchedUsage"/>) reimplemented in an in-memory Doublets store
/// (<c>UnitedMemoryLinks</c> with external references).
/// </summary>
/// <remarks>
/// Numbers (keys, values and members) from 1 to <see cref="MaxNumber"/> are stored as external references (raw numbers,
/// see <see cref="Hybrid{T}"/>), which never collide with link ids. Doublets keeps every <c>(source, target)</c> pair
/// once and indexes links by source and by target, so a lookup by key or by member is a search of an index tree, and
/// every other step follows a link by its id.
/// <list type="table">
/// <item><term>key-value</term><description>the link <c>(key, value)</c>; only these links have a raw number as the
/// source, so <c>GET key</c> is the search <c>(*, key, *)</c></description></item>
/// <item><term>set</term><description>the link <c>(set, member)</c>, where <c>set</c> is a point link created by
/// <see cref="Fork"/>; <c>SISMEMBER</c> is the search of the pair <c>(set, member)</c></description></item>
/// <item><term>queue</term><description>a singly linked list: the node of a value is the link <c>(next, value)</c>,
/// and the head link <c>queue = (first, last)</c> points to its ends; the last node points to <c>queue</c> instead
/// of a next node, and an empty queue is the point <c>(queue, queue)</c>. <c>RPUSH</c> and <c>LPOP</c> follow ids
/// only; <c>RPOP</c> (only used to undo <c>RPUSH</c>) finds the node before the last one by the search
/// <c>(*, last, *)</c></description></item>
/// </list>
/// The order of the updates keeps every pair unique at every step: a new node is created as a point and gets its
/// pair only when no other link can have it.
/// <para>
/// Only <c>UnitedMemoryLinks</c> is used: the <c>SplitMemoryLinks</c> of Platform.Data.Doublets 0.18.1 does not find
/// links whose source is a link and whose target is a raw number, such as the set members
/// (see <c>experiments/csharp-external-references</c>). The index trees are the default ones: the set links share one
/// source, and no tree type gets slower with the number of members (see <c>experiments/csharp-usage-index-trees</c>).
/// </para>
/// </remarks>
public sealed class DoubletsUsage : IBenchedUsage
{
    public const string Variant = "Doublets_United_Volatile";

    private ILinks<ulong> _links = Open();
    private ulong _set;
    private ulong _queue;

    private static ILinks<ulong> Open() => new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory(),
        UnitedMemoryLinks<ulong>.DefaultLinksSizeStep, new LinksConstants<ulong>(enableExternalReferencesSupport: true),
        IndexTreeType.Default);

    /// <summary>The largest number that can be stored: <see cref="long.MaxValue"/>, the largest value of a Redis counter.</summary>
    public const ulong MaxNumber = long.MaxValue;

    /// <summary>The raw number (external reference) of <paramref name="number"/>, which must be in
    /// <c>1..=</c><see cref="MaxNumber"/>, as in the Rust benchmarks.</summary>
    private static ulong Raw(ulong number) => number is >= 1 and <= MaxNumber
        ? new Hybrid<ulong>(number, isExternal: true).Value
        : throw new ArgumentOutOfRangeException(nameof(number), number, $"Doublets stores the numbers 1..={MaxNumber}");

    private static ulong Number(ulong raw) => (ulong)new Hybrid<ulong>(raw, isExternal: true).AbsoluteValue;

    /// <summary>Creates the set and the queue head links and then the background data.</summary>
    public void Fork(int background)
    {
        _set = _links.CreatePoint();
        _queue = _links.CreatePoint();
        for (ulong number = 1; number <= (ulong)background; number++)
        {
            _links.CreateAndUpdate(Raw(number), Raw(number));
        }
        for (ulong number = 1; number <= (ulong)background; number++)
        {
            Push(number);
        }
        for (ulong number = 1; number <= (ulong)background; number++)
        {
            _links.CreateAndUpdate(_set, Raw(number));
        }
    }

    /// <summary>Replaces the store with a new, empty one.</summary>
    public void Unfork()
    {
        Dispose();
        _links = Open();
        (_set, _queue) = (0, 0);
    }

    /// <summary>The first link that matches the query, or 0.</summary>
    private ulong First(ulong source, ulong target)
    {
        var constants = _links.Constants;
        var found = constants.Null;
        _links.Each(link =>
        {
            found = link![0];
            return constants.Break;
        }, constants.Any, source, target);
        return found;
    }

    private ulong Find(ulong key) => First(Raw(key), _links.Constants.Any);

    public void Set(ulong key, ulong value)
    {
        var link = Find(key);
        if (link == 0)
        {
            _links.CreateAndUpdate(Raw(key), Raw(value));
        }
        else
        {
            _links.Update(link, Raw(key), Raw(value));
        }
    }

    public void Get(ulong key, Action<ulong> found)
    {
        var link = Find(key);
        if (link != 0)
        {
            found(Number(_links.GetTarget(link)));
        }
    }

    public void Delete(ulong key)
    {
        var link = Find(key);
        if (link != 0)
        {
            _links.Delete(link, handler: null);
        }
    }

    public void Increment(ulong key)
    {
        var link = Find(key);
        if (link == 0)
        {
            _links.CreateAndUpdate(Raw(key), Raw(1));
        }
        else
        {
            _links.Update(link, Raw(key), Raw(Number(_links.GetTarget(link)) + 1));
        }
    }

    public void Push(ulong value)
    {
        var (first, last) = (_links.GetSource(_queue), _links.GetTarget(_queue));
        var node = _links.CreatePoint();
        if (first == _queue)
        {
            _links.Update(node, _queue, Raw(value));
            _links.Update(_queue, node, node);
        }
        else
        {
            _links.Update(last, node, _links.GetTarget(last));
            _links.Update(node, _queue, Raw(value));
            _links.Update(_queue, first, node);
        }
    }

    public void Pop(Action<ulong> popped)
    {
        var (first, last) = (_links.GetSource(_queue), _links.GetTarget(_queue));
        if (first == _queue)
        {
            return;
        }
        var (next, value) = (_links.GetSource(first), _links.GetTarget(first));
        if (next == _queue)
        {
            _links.Update(_queue, _queue, _queue);
        }
        else
        {
            _links.Update(_queue, next, last);
        }
        _links.Delete(first, handler: null);
        popped(Number(value));
    }

    public void PushFront(ulong value)
    {
        var (first, last) = (_links.GetSource(_queue), _links.GetTarget(_queue));
        var node = _links.CreatePoint();
        _links.Update(node, first, Raw(value));
        _links.Update(_queue, node, first == _queue ? node : last);
    }

    public void PopBack()
    {
        var (first, last) = (_links.GetSource(_queue), _links.GetTarget(_queue));
        if (first == _queue)
        {
            return;
        }
        if (first == last)
        {
            _links.Update(_queue, _queue, _queue);
        }
        else
        {
            // The node before the last one is the only link whose source is the last node:
            // the head link starts at the first node, and all other links start at a raw number or at the set.
            var previous = First(last, _links.Constants.Any);
            _links.Update(_queue, first, previous);
            _links.Delete(last, handler: null);
            _links.Update(previous, _queue, _links.GetTarget(previous));
            return;
        }
        _links.Delete(last, handler: null);
    }

    private ulong FindMember(ulong member) => _links.SearchOrDefault(_set, Raw(member));

    public void Add(ulong member)
    {
        if (FindMember(member) == 0)
        {
            _links.CreateAndUpdate(_set, Raw(member));
        }
    }

    public void Remove(ulong member)
    {
        var link = FindMember(member);
        if (link != 0)
        {
            _links.Delete(link, handler: null);
        }
    }

    public void Contains(ulong member, Action<bool> result) => result(FindMember(member) != 0);

    public UsageSnapshot Snapshot()
    {
        var any = _links.Constants.Any;
        var values = new List<(ulong, ulong)>();
        var members = new List<ulong>();
        _links.Each(link =>
        {
            var (source, target) = (link![1], link[2]);
            if (_links.Constants.IsExternalReference(source))
            {
                values.Add((Number(source), Number(target)));
            }
            else if (source == _set && _links.Constants.IsExternalReference(target))
            {
                members.Add(Number(target));
            }
            return _links.Constants.Continue;
        }, any, any, any);
        var queue = new List<ulong>();
        for (var node = _queue == 0 ? _queue : _links.GetSource(_queue); node != _queue; node = _links.GetSource(node))
        {
            queue.Add(Number(_links.GetTarget(node)));
        }
        return new(values.Order().ToList(), queue, members.Order().ToList());
    }

    public void Dispose() => (_links as IDisposable)?.Dispose();
}
