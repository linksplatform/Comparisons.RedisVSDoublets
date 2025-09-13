using System.Text.Json;
using StackExchange.Redis;
using Comparisons.RedisVSDoublets.Model;

namespace Comparisons.RedisVSDoublets.Redis
{
    /// <summary>
    /// Redis implementation of TestRun
    /// </summary>
    public class RedisTestRun : TestRun
    {
        private ConnectionMultiplexer? _connection;
        private IDatabase? _database;
        private const string BlogPostsKey = "blogposts";

        /// <summary>
        /// Initializes a new instance of the RedisTestRun class
        /// </summary>
        /// <param name="connectionString">Redis connection string</param>
        public RedisTestRun(string connectionString) : base(connectionString)
        {
        }

        /// <summary>
        /// Prepares the Redis database
        /// </summary>
        protected override void Prepare()
        {
            _connection = ConnectionMultiplexer.Connect(DbFilename);
            _database = _connection.GetDatabase();
            
            // Clear the database
            var server = _connection.GetServer(_connection.GetEndPoints()[0]);
            server.FlushDatabase();
        }

        /// <summary>
        /// Creates the list in Redis
        /// </summary>
        protected override void CreateList()
        {
            if (_database == null) return;

            var blogPostsJson = JsonSerializer.Serialize(BlogPosts.List);
            _database.StringSet(BlogPostsKey, blogPostsJson);
        }

        /// <summary>
        /// Reads the list from Redis
        /// </summary>
        protected override void ReadList()
        {
            if (_database == null) return;

            var blogPostsJson = _database.StringGet(BlogPostsKey);
            if (blogPostsJson.HasValue)
            {
                var blogPosts = JsonSerializer.Deserialize<List<BlogPost>>(blogPostsJson!);
                if (blogPosts != null)
                {
                    ReadBlogPosts.AddRange(blogPosts);
                }
            }
        }

        /// <summary>
        /// Deletes the list from Redis
        /// </summary>
        protected override void DeleteList()
        {
            if (_database == null) return;

            _database.KeyDelete(BlogPostsKey);
            ReadBlogPosts.Clear();
        }

        /// <summary>
        /// Gets the database size in bytes (Redis memory usage)
        /// </summary>
        /// <returns>Database size in bytes</returns>
        protected override long GetDatabaseSizeInBytes()
        {
            if (_connection == null) return 0;

            try
            {
                var server = _connection.GetServer(_connection.GetEndPoints()[0]);
                var info = server.Info("memory");
                var memorySection = info.FirstOrDefault(i => i.Key == "memory");
                if (memorySection != default)
                {
                    var usedMemory = memorySection.FirstOrDefault(kv => kv.Key == "used_memory");
                    if (usedMemory != default && long.TryParse(usedMemory.Value, out var memory))
                    {
                        return memory;
                    }
                }
            }
            catch
            {
                // If we can't get memory info, return 0
            }
            
            return 0;
        }

        /// <summary>
        /// Disposes the Redis connection
        /// </summary>
        protected override void DeleteDatabase()
        {
            if (_connection != null)
            {
                var server = _connection.GetServer(_connection.GetEndPoints()[0]);
                server.FlushDatabase();
            }
            
            _connection?.Dispose();
            _connection = null;
            _database = null;
        }
    }
}