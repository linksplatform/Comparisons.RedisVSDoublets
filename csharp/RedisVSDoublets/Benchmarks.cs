namespace Comparisons.RedisVSDoublets;

/// <summary>The measured work and its undo, both run on the store of the benchmark.</summary>
public sealed record Work<TStore>(Action<TStore> Operation, Action<TStore> Undo);

/// <summary>
/// The operations of the links benchmarks, the same as in <c>rust/benches/benchmarks/links.rs</c>. They are written once
/// and run unchanged on every store, so Redis and Doublets execute exactly the same sequence of calls.
/// </summary>
/// <remarks>
/// <c>B</c> is <see cref="Settings.BackgroundLinks"/> and <c>N</c> is <see cref="Settings.BenchmarkLinks"/>.
/// Before every iteration the store contains the point links <c>1..=B</c>.
/// <list type="table">
/// <item><term>Create</term><description><c>N</c> × <c>CreatePoint()</c>, which creates links <c>B + 1..=B + N</c>; undone by deleting them from the highest id</description></item>
/// <item><term>Update</term><description><c>N</c> × (<c>Update(id, 0, 0)</c> then <c>Update(id, id, id)</c>) for the links <c>B - N + 1..=B</c>; nothing to undo</description></item>
/// <item><term>Delete</term><description><c>N</c> × <c>Delete(id)</c>, from link <c>B</c> down to link <c>B - N + 1</c>; undone by <c>N</c> × <c>CreatePoint()</c></description></item>
/// <item><term>Each_All</term><description>1 × <c>Each(*, *, *)</c>, which visits all <c>B</c> links</description></item>
/// <item><term>Each_Identity</term><description><c>N</c> × <c>Each(id, *, *)</c> for the ids <c>1..=N</c></description></item>
/// <item><term>Each_Concrete</term><description><c>N</c> × <c>Each(*, id, id)</c></description></item>
/// <item><term>Each_Outgoing</term><description><c>N</c> × <c>Each(*, id, *)</c></description></item>
/// <item><term>Each_Incoming</term><description><c>N</c> × <c>Each(*, *, id)</c></description></item>
/// </list>
/// </remarks>
public static class Benchmarks
{
    /// <summary>The benchmark groups, in the order in which the Rust benchmarks run them.</summary>
    public static readonly string[] Groups =
        ["Create", "Delete", "Each_Identity", "Each_Concrete", "Each_Outgoing", "Each_Incoming", "Each_All", "Update"];

    /// <summary>Every found link id (and every value read by the usage benchmarks) is added to this value, so reading it cannot be skipped.</summary>
    public static ulong Visited { get; set; }

    private static void Visit(Link link) => Visited += link.Id;

    /// <summary>The query <c>(id, source, target)</c> of an <c>Each_*</c> group for a link id and the value that matches everything.</summary>
    public static Func<ulong, ulong, (ulong Id, ulong Source, ulong Target)>? Query(string group) => group switch
    {
        "Each_Identity" => (id, any) => (id, any, any),
        "Each_Concrete" => (id, any) => (any, id, id),
        "Each_Outgoing" => (id, any) => (any, id, any),
        "Each_Incoming" => (id, any) => (any, any, id),
        _ => null,
    };

    /// <summary>The work of <paramref name="group"/> as calls of single operations.</summary>
    public static Work<IBenchedLinks> Single(string group, int backgroundLinks, int links)
    {
        var (background, count) = ((ulong)backgroundLinks, (ulong)links);
        static void Nothing(IBenchedLinks store)
        {
        }

        return group switch
        {
            "Create" => new(
                store =>
                {
                    for (ulong i = 0; i < count; i++)
                    {
                        store.CreatePoint();
                    }
                },
                store =>
                {
                    for (var id = background + count; id > background; id--)
                    {
                        store.Delete(id);
                    }
                }),
            "Update" => new(
                store =>
                {
                    for (var id = background - count + 1; id <= background; id++)
                    {
                        store.Update(id, 0, 0);
                        store.Update(id, id, id);
                    }
                },
                Nothing),
            "Delete" => new(
                store =>
                {
                    for (var id = background; id > background - count; id--)
                    {
                        store.Delete(id);
                    }
                },
                store =>
                {
                    for (ulong i = 0; i < count; i++)
                    {
                        store.CreatePoint();
                    }
                }),
            "Each_All" => new(store => store.Each(store.Any, store.Any, store.Any, Visit), Nothing),
            _ when Query(group) is { } query => new(
                store =>
                {
                    for (ulong id = 1; id <= count; id++)
                    {
                        var (index, source, target) = query(id, store.Any);
                        store.Each(index, source, target, Visit);
                    }
                },
                Nothing),
            _ => throw new ArgumentException($"unknown benchmark group {group}", nameof(group)),
        };
    }
}
