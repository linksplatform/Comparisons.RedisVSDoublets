using System.Globalization;
using System.Reflection;
using Platform.Data;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;
using StackExchange.Redis;

namespace Comparisons.RedisVSDoublets;

public readonly record struct Link(ulong Id, ulong Source, ulong Target);

public interface IStore : IDisposable
{
    ulong Create();
    void Update(ulong id, ulong source, ulong target);
    void Delete(ulong id);
    void Each(ulong id, ulong source, ulong target, Action<Link> visit);
}

public sealed class DoubletsStore : IStore
{
    private readonly ILinks<ulong> _links = new UnitedMemoryLinks<ulong>(new HeapResizableDirectMemory());
    public ulong Create() => _links.CreatePoint();
    public void Update(ulong id, ulong source, ulong target)
    {
        EnsureExists(id);
        _links.Update(id, source, target);
    }
    public void Delete(ulong id)
    {
        EnsureExists(id);
        _links.Delete(id, handler: null);
    }
    private void EnsureExists(ulong id)
    {
        if (!_links.Exists(id)) throw new InvalidOperationException($"missing link {id}");
    }
    public void Each(ulong id, ulong source, ulong target, Action<Link> visit) => _links.Each(link =>
    {
        visit(new Link(link![0], link[1], link[2]));
        return _links.Constants.Continue;
    }, Part(id), Part(source), Part(target));
    private ulong Part(ulong value) => value == ulong.MaxValue ? _links.Constants.Any : value;
    public void Dispose() => (_links as IDisposable)?.Dispose();
}

public sealed class RedisStore : IStore
{
    private readonly ConnectionMultiplexer _connection;
    private readonly IDatabase _database;
    private readonly RedisKey[] _keys = [$"redis-doublets:{Guid.NewGuid():N}:"];
    private static readonly string Mutate = Script("mutate");
    private static readonly string Query = Script("each");
    private static readonly string Clear = Script("clear");
    private bool _disposed;

    public RedisStore()
    {
        var url = Environment.GetEnvironmentVariable("REDIS_URL") ?? "redis://127.0.0.1:6379";
        // Accept the same redis:// endpoint as Rust, including a database and password.
        var uri = new Uri(url);
        if (uri.Scheme != "redis") throw new ArgumentException("REDIS_URL must use redis://");
        var options = new ConfigurationOptions { AbortOnConnectFail = true, ConnectTimeout = 10000, SyncTimeout = 30000 };
        options.EndPoints.Add(uri.Host, uri.IsDefaultPort ? 6379 : uri.Port);
        if (uri.UserInfo.Length != 0)
        {
            var credentials = uri.UserInfo.Split(':', 2);
            if (credentials.Length == 2 && credentials[0].Length != 0) options.User = Uri.UnescapeDataString(credentials[0]);
            options.Password = Uri.UnescapeDataString(credentials[^1]);
        }
        var database = uri.AbsolutePath.Trim('/');
        if (database.Length != 0) options.DefaultDatabase = int.Parse(database, CultureInfo.InvariantCulture);
        _connection = ConnectionMultiplexer.Connect(options);
        _database = _connection.GetDatabase();
    }

    private static string Script(string name)
    {
        var assembly = Assembly.GetExecutingAssembly();
        using var stream = assembly.GetManifestResourceStream($"RedisVSDoublets.{name}.lua")
            ?? throw new InvalidOperationException($"missing script {name}");
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }

    public string Version => _connection.GetServer(_connection.GetEndPoints()[0]).Version.ToString();

    private ulong Mutation(string mode, ulong id, ulong source, ulong target) =>
        checked((ulong)(long)_database.ScriptEvaluate(Mutate, _keys, [mode, id, source, target]));
    public ulong Create() => Mutation("create", 0, 0, 0);
    public void Update(ulong id, ulong source, ulong target) => Mutation("update", id, source, target);
    public void Delete(ulong id) => Mutation("delete", id, 0, 0);
    public void Each(ulong id, ulong source, ulong target, Action<Link> visit)
    {
        static RedisValue Part(ulong value) => value == ulong.MaxValue ? "*" : value.ToString(CultureInfo.InvariantCulture);
        var result = (RedisResult[]?)_database.ScriptEvaluate(Query, _keys, [Part(id), Part(source), Part(target)])
            ?? throw new InvalidOperationException("missing Redis result");
        for (var i = 0; i < result.Length; i += 3)
            visit(new Link(checked((ulong)(long)result[i]), checked((ulong)(long)result[i + 1]), checked((ulong)(long)result[i + 2])));
    }
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        try { _database.ScriptEvaluate(Clear, _keys); }
        finally { _connection.Dispose(); }
    }
}
