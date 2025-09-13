using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;

namespace Comparisons.RedisVSDoublets.Model
{
    public static class BlogPosts
    {
        public static IReadOnlyList<BlogPost> List { get; private set; } = new List<BlogPost>();

        static BlogPosts()
        {
            List = new List<BlogPost>();
        }

        public static void GenerateData(int numberOfRecords)
        {
            var blogPosts = new List<BlogPost>();
            var random = new Random();

            var contentVariations = new[]
            {
                "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
                "Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
                "Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.",
                "Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.",
                "Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium.",
                "Totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt.",
                "Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores.",
                "Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit."
            };

            for (int i = 1; i <= numberOfRecords; i++)
            {
                blogPosts.Add(new BlogPost
                {
                    Id = i,
                    Title = $"Blog post {i}",
                    Content = contentVariations[random.Next(contentVariations.Length)],
                    PublicationDateTime = DateTime.Now.AddDays(-random.Next(30))
                });
            }

            List = new ReadOnlyCollection<BlogPost>(blogPosts);
        }
    }
}