using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using Comparisons.RedisVSDoublets.Model;

namespace Comparisons.RedisVSDoublets
{
    /// <summary>
    /// Abstract base class for test runs
    /// </summary>
    public abstract class TestRun
    {
        /// <summary>
        /// Database filename or connection string
        /// </summary>
        public string DbFilename { get; }

        /// <summary>
        /// Test run results
        /// </summary>
        public TestRunResults Results { get; }

        /// <summary>
        /// List of blog posts read from the database
        /// </summary>
        public List<BlogPost> ReadBlogPosts { get; }

        /// <summary>
        /// Initializes a new instance of the TestRun class
        /// </summary>
        /// <param name="dbFilename">Database filename or connection string</param>
        protected TestRun(string dbFilename)
        {
            DbFilename = dbFilename;
            Results = new TestRunResults();
            ReadBlogPosts = new List<BlogPost>();
        }

        /// <summary>
        /// Runs the test
        /// </summary>
        public void Run()
        {
            DeleteDatabase();

            var sw = Stopwatch.StartNew();
            Prepare();
            sw.Stop();
            Results.PrepareTime = sw.Elapsed;
            Results.DbSizeAfterPrepare = GetDatabaseSizeInBytes();

            sw.Restart();
            CreateList();
            sw.Stop();
            Results.ListCreationTime = sw.Elapsed;
            Results.DbSizeAfterCreation = GetDatabaseSizeInBytes();

            sw.Restart();
            ReadList();
            sw.Stop();
            Results.ListReadingTime = sw.Elapsed;
            Results.DbSizeAfterReading = GetDatabaseSizeInBytes();

            sw.Restart();
            DeleteList();
            sw.Stop();
            Results.ListDeletionTime = sw.Elapsed;
            Results.DbSizeAfterDeletion = GetDatabaseSizeInBytes();

            DeleteDatabase();
        }

        /// <summary>
        /// Prepares the database
        /// </summary>
        protected abstract void Prepare();

        /// <summary>
        /// Creates the list in the database
        /// </summary>
        protected abstract void CreateList();

        /// <summary>
        /// Reads the list from the database
        /// </summary>
        protected abstract void ReadList();

        /// <summary>
        /// Deletes the list from the database
        /// </summary>
        protected abstract void DeleteList();

        /// <summary>
        /// Gets the database size in bytes
        /// </summary>
        /// <returns>Database size in bytes</returns>
        protected virtual long GetDatabaseSizeInBytes()
        {
            return 0;
        }

        /// <summary>
        /// Deletes the database
        /// </summary>
        protected virtual void DeleteDatabase()
        {
            if (File.Exists(DbFilename))
            {
                File.Delete(DbFilename);
            }
        }
    }
}