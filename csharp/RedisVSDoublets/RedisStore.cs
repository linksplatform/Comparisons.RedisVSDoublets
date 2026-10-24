using System.Reflection;
using StackExchange.Redis;

namespace Comparisons.RedisVSDoublets;

/// <summary>How the commands of one benchmark iteration are sent to Redis.</summary>
public enum Mode
{
    /// <summary>Every operation sends its command and waits for the reply before the next operation: one round trip each.</summary>
    NonTransaction,

    /// <summary>
    /// The commands of an iteration are queued between <c>MULTI</c> and <c>EXEC</c> and run atomically at
    /// <see cref="IBenched.Commit"/>: one round trip for the whole iteration.
    /// </summary>
    Transaction,

    /// <summary>
    /// The commands of an iteration are sent together at <see cref="IBenched.Commit"/>, without waiting for each reply
    /// (pipelining), and run one by one, not atomically.
    /// </summary>
    Pipeline,
}

/// <summary>
/// The connection to Redis shared by <see cref="RedisLinks"/> and <see cref="RedisUsage"/>, through the
/// <see href="https://www.nuget.org/packages/StackExchange.Redis">StackExchange.Redis</see> client, and the
/// handling of the <see cref="Mode"/>.
/// </summary>
/// <remarks>
/// The connection is opened once and reused by all operations. Every operation calls the asynchronous API of the
/// client; in <see cref="Mode.NonTransaction"/> it waits for the reply at once, in the other modes the replies are
/// collected and their callbacks run in <see cref="Commit"/>, in the order of the operations.
/// <para>
/// <see cref="Unfork"/> runs <c>FLUSHDB</c>: the benchmarks and the tests empty the database selected by
/// <c>REDIS_URL</c>, so they must use a Redis server that keeps no other data.
/// </para>
/// </remarks>
public abstract class RedisStore : IBenched
{
    /// <summary>Number of links, keys, list items or set members created by one command of <see cref="IBenched.Fork"/>.</summary>
    public const int BatchSize = 10_000;

    private readonly ConnectionMultiplexer _connection;
    private readonly List<Action> _replies = [];
    private IBatch? _batch;

    protected RedisStore(string url, Mode mode)
    {
        _connection = ConnectionMultiplexer.Connect(Options(url));
        Database = _connection.GetDatabase();
        Mode = mode;
        Database.Execute("FUNCTION", "LOAD", "REPLACE", Functions);
    }

    public Mode Mode { get; }

    /// <summary>The database, for commands that run at once, outside of <see cref="Begin"/> and <see cref="Commit"/>.</summary>
    protected IDatabase Database { get; }

    /// <summary>Where the commands of an operation go: the current transaction or pipeline, or the database.</summary>
    protected IDatabaseAsync Commands => _batch ?? (IDatabaseAsync)Database;

    /// <summary>The functions of <c>redis/links.lua</c>, shared with the Rust benchmarks.</summary>
    public static string Functions { get; } = new StreamReader(
        Assembly.GetExecutingAssembly().GetManifestResourceStream("links.lua")!).ReadToEnd();

    /// <summary><c>REDIS_URL</c>, by default <c>redis://localhost:6379</c>, as for the Rust benchmarks.</summary>
    public static string Url => Settings.Variable("REDIS_URL", "redis://localhost:6379");

    /// <summary>
    /// Reads a <c>redis://[[user]:password@]host[:port][/database]</c> URL, the format of the Rust client,
    /// into the options of StackExchange.Redis.
    /// </summary>
    public static ConfigurationOptions Options(string url)
    {
        var uri = new Uri(url);
        if (uri.Scheme is not ("redis" or "rediss"))
        {
            throw new ArgumentException($"REDIS_URL must start with redis:// or rediss://, got `{url}`");
        }
        var options = new ConfigurationOptions
        {
            EndPoints = { { uri.Host, uri.IsDefaultPort ? 6379 : uri.Port } },
            Ssl = uri.Scheme == "rediss",
            // FLUSHDB is an admin command.
            AllowAdmin = true,
            AbortOnConnectFail = true,
        };
        if (uri.UserInfo.Split(':', 2) is [var user, var password])
        {
            options.User = user.Length > 0 ? Uri.UnescapeDataString(user) : null;
            options.Password = Uri.UnescapeDataString(password);
        }
        if (uri.AbsolutePath.Trim('/') is { Length: > 0 } database)
        {
            options.DefaultDatabase = int.Parse(database);
        }
        return options;
    }

    public abstract void Fork(int background);

    public void Begin()
    {
        _batch = Mode switch
        {
            Mode.Transaction => Database.CreateTransaction(),
            Mode.Pipeline => Database.CreateBatch(),
            _ => null,
        };
    }

    public void Commit()
    {
        if (_batch is not { } batch)
        {
            return;
        }
        _batch = null;
        if (batch is ITransaction transaction)
        {
            // Without conditions EXEC always runs the queued commands; an error of a command faults only its own reply.
            if (!transaction.Execute())
            {
                throw new InvalidOperationException("the transaction was not executed");
            }
        }
        else
        {
            batch.Execute();
        }
        try
        {
            foreach (var reply in _replies)
            {
                reply();
            }
        }
        finally
        {
            _replies.Clear();
        }
    }

    /// <summary>Removes all keys of the database. The functions stay loaded.</summary>
    public virtual void Unfork()
    {
        _batch = null;
        _replies.Clear();
        Database.Execute("FLUSHDB");
    }

    /// <summary>Passes the reply of <paramref name="command"/> to <paramref name="then"/>: at once, or in <see cref="Commit"/>.</summary>
    protected void Reply<T>(Task<T> command, Action<T> then)
    {
        if (_batch is null)
        {
            then(Wait(command));
        }
        else
        {
            _replies.Add(() => then(Wait(command)));
        }
    }

    /// <summary>Waits for <paramref name="command"/> (at once, or in <see cref="Commit"/>), so that its errors are not lost.</summary>
    protected void Reply(Task command)
    {
        if (_batch is null)
        {
            command.GetAwaiter().GetResult();
        }
        else
        {
            _replies.Add(() => command.GetAwaiter().GetResult());
        }
    }

    protected static T Wait<T>(Task<T> task) => task.GetAwaiter().GetResult();

    /// <summary>Splits <c>first..=last</c> into ranges of <see cref="BatchSize"/> numbers, so that no command gets too large.</summary>
    protected static IEnumerable<(ulong First, ulong Last)> Chunks(ulong first, ulong last)
    {
        for (var start = first; start <= last; start += BatchSize)
        {
            yield return (start, Math.Min(last, start + BatchSize - 1));
        }
    }

    /// <summary>The version of the Redis server, from <c>INFO server</c>.</summary>
    public string ServerVersion() => _connection.GetServers()[0].Version.ToString();

    public void Dispose()
    {
        _connection.Dispose();
        GC.SuppressFinalize(this);
    }
}
