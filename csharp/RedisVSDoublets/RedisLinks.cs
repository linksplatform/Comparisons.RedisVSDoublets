using Platform.Data;
using StackExchange.Redis;

namespace Comparisons.RedisVSDoublets;

/// <summary>
/// A links store kept in Redis. Changes run the functions of <c>redis/links.lua</c> (<c>FCALL</c>), which update a link
/// and its indexes atomically in one round trip; queries are single native read commands.
/// </summary>
/// <remarks>
/// <list type="table">
/// <item><term><c>links</c></term><description>hash <c>id -> "source:target"</c>: <c>Each(id, *, *)</c> is <c>HGET</c>,
/// <c>Each(*, *, *)</c> is <c>HGETALL</c></description></item>
/// <item><term><c>out:{source}</c></term><description>hash <c>id -> target</c>: <c>Each(*, source, *)</c> is <c>HGETALL</c></description></item>
/// <item><term><c>in:{target}</c></term><description>hash <c>id -> source</c>: <c>Each(*, *, target)</c> is <c>HGETALL</c></description></item>
/// <item><term><c>pair:{source}:{target}</c></term><description>set of ids: <c>Each(*, source, target)</c> is <c>SMEMBERS</c></description></item>
/// </list>
/// New ids come from a counter kept by the client, as with a database sequence, so both databases assign the same ids:
/// <c>1, 2, 3, ...</c>, and deleting the link with the highest id makes that id the next one to be created.
/// </remarks>
public sealed class RedisLinks : RedisStore, IBenchedLinks
{
    private ulong _nextId = 1;

    private RedisLinks(string url, Mode mode) : base(url, mode)
    {
    }

    /// <summary>The same value as in the Doublets stores.</summary>
    public ulong Any { get; } = new LinksConstants<ulong>(enableExternalReferencesSupport: false).Any;

    /// <summary>Connects to <paramref name="url"/>, loads the functions and removes all keys of the database.</summary>
    public static RedisLinks Open(Mode mode, string? url = null)
    {
        var store = new RedisLinks(url ?? Url, mode);
        store.Unfork();
        return store;
    }

    /// <summary>Creates the point links with the next ids, <see cref="RedisStore.BatchSize"/> links per function call.</summary>
    public override void Fork(int background)
    {
        var last = _nextId + (ulong)background - 1;
        foreach (var (first, end) in Chunks(_nextId, last))
        {
            Database.Execute("FCALL", "link_points", 0, first, end);
        }
        _nextId = last + 1;
    }

    public override void Unfork()
    {
        base.Unfork();
        _nextId = 1;
    }

    public ulong CreatePoint()
    {
        var id = _nextId++;
        Reply(Commands.ExecuteAsync("FCALL", "link_create", 0, id, id, id));
        return id;
    }

    public void Update(ulong id, ulong source, ulong target) =>
        Reply(Commands.ExecuteAsync("FCALL", "link_update", 0, id, source, target));

    public void Delete(ulong id)
    {
        Reply(Commands.ExecuteAsync("FCALL", "link_delete", 0, id));
        if (id + 1 == _nextId)
        {
            _nextId = id;
        }
    }

    public void Each(ulong id, ulong source, ulong target, Action<Link> visit)
    {
        if (id != Any)
        {
            Reply(Commands.HashGetAsync("links", id), link =>
            {
                if (!link.IsNull && Parse(id, link) is var found
                    && (source == Any || source == found.Source) && (target == Any || target == found.Target))
                {
                    visit(found);
                }
            });
        }
        else if (source != Any && target != Any)
        {
            Reply(Commands.SetMembersAsync($"pair:{source}:{target}"), ids =>
            {
                foreach (var found in ids)
                {
                    visit(new Link((ulong)found, source, target));
                }
            });
        }
        else if (source != Any)
        {
            Reply(Commands.HashGetAllAsync($"out:{source}"), links =>
            {
                foreach (var link in links)
                {
                    visit(new Link((ulong)link.Name, source, (ulong)link.Value));
                }
            });
        }
        else if (target != Any)
        {
            Reply(Commands.HashGetAllAsync($"in:{target}"), links =>
            {
                foreach (var link in links)
                {
                    visit(new Link((ulong)link.Name, (ulong)link.Value, target));
                }
            });
        }
        else
        {
            Reply(Commands.HashGetAllAsync("links"), links =>
            {
                foreach (var link in links)
                {
                    visit(Parse((ulong)link.Name, link.Value));
                }
            });
        }
    }

    public ulong Count() => (ulong)Database.HashLength("links");

    /// <summary>Reads the <c>"source:target"</c> value of the link <paramref name="id"/>.</summary>
    private static Link Parse(ulong id, RedisValue link)
    {
        var text = ((string?)link).AsSpan();
        var colon = text.IndexOf(':');
        return new Link(id, ulong.Parse(text[..colon]), ulong.Parse(text[(colon + 1)..]));
    }
}
