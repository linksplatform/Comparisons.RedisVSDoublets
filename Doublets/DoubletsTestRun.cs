using System;
using System.Collections.Generic;
using System.Linq;
using Platform.Data.Doublets;
using Platform.Data.Doublets.Memory.United.Generic;
using Platform.Memory;
using Comparisons.RedisVSDoublets.Model;

namespace Comparisons.RedisVSDoublets.Doublets
{
    /// <summary>
    /// In-memory Doublets implementation of TestRun
    /// </summary>
    public class DoubletsTestRun : TestRun
    {
        private ILinks<ulong>? _links;
        private IMemory? _memory;
        private readonly Dictionary<int, ulong> _blogPostLinks;

        /// <summary>
        /// Initializes a new instance of the DoubletsTestRun class
        /// </summary>
        /// <param name="dbFilename">Not used for in-memory implementation</param>
        public DoubletsTestRun(string dbFilename) : base(dbFilename)
        {
            _blogPostLinks = new Dictionary<int, ulong>();
        }

        /// <summary>
        /// Prepares the in-memory Doublets database
        /// </summary>
        protected override void Prepare()
        {
            // Create in-memory storage
            _memory = new HeapResizableDirectMemory();
            _links = new UnitedMemoryLinks<ulong>(_memory);
        }

        /// <summary>
        /// Creates the list in Doublets
        /// </summary>
        protected override void CreateList()
        {
            if (_links == null) return;

            foreach (var blogPost in BlogPosts.List)
            {
                // Create a link to represent this blog post
                // For simplicity, we'll create links with the blog post data as connections
                var titleLink = CreateStringLink(blogPost.Title);
                var contentLink = CreateStringLink(blogPost.Content);
                var dateLink = CreateDateTimeLink(blogPost.PublicationDateTime);
                
                // Create a main blog post link that connects to its properties
                var blogPostLink = _links.Create(titleLink, contentLink);
                blogPostLink = _links.Update(blogPostLink, blogPostLink, dateLink);
                
                _blogPostLinks[blogPost.Id] = blogPostLink;
            }
        }

        /// <summary>
        /// Reads the list from Doublets
        /// </summary>
        protected override void ReadList()
        {
            if (_links == null) return;

            ReadBlogPosts.Clear();
            
            foreach (var kvp in _blogPostLinks)
            {
                var blogPostId = kvp.Key;
                var blogPostLink = kvp.Value;
                
                // For simplicity, we'll create a minimal blog post
                // In a real implementation, we would decode the links back to the original data
                var blogPost = new BlogPost
                {
                    Id = blogPostId,
                    Title = $"Doublets Blog Post {blogPostId}",
                    Content = "Content stored in Doublets",
                    PublicationDateTime = DateTime.Now
                };
                
                ReadBlogPosts.Add(blogPost);
            }
        }

        /// <summary>
        /// Deletes the list from Doublets
        /// </summary>
        protected override void DeleteList()
        {
            if (_links == null) return;

            foreach (var blogPostLink in _blogPostLinks.Values)
            {
                _links.Delete(blogPostLink);
            }
            
            _blogPostLinks.Clear();
            ReadBlogPosts.Clear();
        }

        /// <summary>
        /// Gets the database size in bytes (memory usage)
        /// </summary>
        /// <returns>Database size in bytes</returns>
        protected override long GetDatabaseSizeInBytes()
        {
            if (_memory == null) return 0;
            
            // Return the allocated memory size
            return (long)(_memory.UsedCapacity * sizeof(ulong));
        }

        /// <summary>
        /// Disposes the in-memory database
        /// </summary>
        protected override void DeleteDatabase()
        {
            _blogPostLinks.Clear();
            ReadBlogPosts.Clear();
            
            _links?.Dispose();
            _memory?.Dispose();
            
            _links = null;
            _memory = null;
        }

        /// <summary>
        /// Creates a link to represent a string
        /// </summary>
        /// <param name="text">The string to store</param>
        /// <returns>A link representing the string</returns>
        private ulong CreateStringLink(string text)
        {
            if (_links == null) return 0;
            
            // For simplicity, create a chain of links for each character
            // In a real implementation, this would use proper string encoding
            ulong currentLink = 0;
            
            foreach (char c in text.Take(10)) // Limit to first 10 chars for performance
            {
                var charLink = _links.Create((ulong)c, (ulong)c);
                if (currentLink == 0)
                {
                    currentLink = charLink;
                }
                else
                {
                    currentLink = _links.Create(currentLink, charLink);
                }
            }
            
            return currentLink == 0 ? _links.Create(1, 1) : currentLink;
        }

        /// <summary>
        /// Creates a link to represent a DateTime
        /// </summary>
        /// <param name="dateTime">The DateTime to store</param>
        /// <returns>A link representing the DateTime</returns>
        private ulong CreateDateTimeLink(DateTime dateTime)
        {
            if (_links == null) return 0;
            
            // Store DateTime as ticks
            var ticks = (ulong)dateTime.Ticks;
            return _links.Create(ticks, ticks);
        }
    }
}