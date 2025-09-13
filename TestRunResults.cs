using System;
using System.Text;

namespace Comparisons.RedisVSDoublets
{
    /// <summary>
    /// Represents the results of a test run
    /// </summary>
    public class TestRunResults
    {
        /// <summary>
        /// Time taken to prepare the database
        /// </summary>
        public TimeSpan PrepareTime { get; set; }

        /// <summary>
        /// Database size after preparation
        /// </summary>
        public long DbSizeAfterPrepare { get; set; }

        /// <summary>
        /// Time taken to create the list
        /// </summary>
        public TimeSpan ListCreationTime { get; set; }

        /// <summary>
        /// Database size after list creation
        /// </summary>
        public long DbSizeAfterCreation { get; set; }

        /// <summary>
        /// Time taken to read the list
        /// </summary>
        public TimeSpan ListReadingTime { get; set; }

        /// <summary>
        /// Database size after list reading
        /// </summary>
        public long DbSizeAfterReading { get; set; }

        /// <summary>
        /// Time taken to delete the list
        /// </summary>
        public TimeSpan ListDeletionTime { get; set; }

        /// <summary>
        /// Database size after list deletion
        /// </summary>
        public long DbSizeAfterDeletion { get; set; }

        /// <summary>
        /// Returns a string representation of the test results
        /// </summary>
        public override string ToString()
        {
            var sb = new StringBuilder();
            sb.AppendLine($"Prepare time: {PrepareTime}");
            if (DbSizeAfterPrepare > 0)
                sb.AppendLine($"DB size after prepare: {DbSizeAfterPrepare}");
            sb.AppendLine($"List creation time: {ListCreationTime}");
            if (DbSizeAfterCreation > 0)
                sb.AppendLine($"DB size after creation: {DbSizeAfterCreation}");
            sb.AppendLine($"List reading time: {ListReadingTime}");
            if (DbSizeAfterReading > 0)
                sb.AppendLine($"DB size after reading: {DbSizeAfterReading}");
            sb.AppendLine($"List deletion time: {ListDeletionTime}");
            if (DbSizeAfterDeletion > 0)
                sb.AppendLine($"DB size after deletion: {DbSizeAfterDeletion}");
            return sb.ToString();
        }
    }
}